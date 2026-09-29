//! Purpose:
//! Static property writes and static array mutations.
//!
//! Called from:
//! - `crate::ir_lower::stmt`.
//!
//! Key details:
//! - Preserves statement ordering, CFG shape, EIR effects, and ownership contracts.

use super::*;

/// Lowers a static property write.
///
/// A static property outlives the enclosing scope, so it must hold its own
/// reference to a refcounted value. There are two storage disciplines, matched
/// to what the codegen store actually does:
///
/// - **Independent-value store** (the store crosses the boxed/unboxed boundary, in
///   either direction): a Mixed/Union slot receiving a non-Mixed value, e.g.
///   `Class::$h = new C()`, is boxed with `__rt_mixed_from_value`, which takes its
///   *own* retained reference to the child; and a narrowing slot receiving a Mixed
///   value, e.g. `Class::$s += 1` on an `int` slot, is cast out of the box, so the
///   slot holds a payload word that owns nothing of it. Either way the slot keeps a
///   reference independent of the source, so an owning temporary must be *released*
///   after the store (its reference is not the one the slot holds), and a borrowed
///   source must be left untouched. Acquiring here would leak the extra reference on
///   top of the slot's, and skipping the release leaks the source's own -- which is
///   what made every compound assignment to a typed static property leak one Mixed
///   cell (issue #1041).
/// - **Moving store** (every other case: a same-representation store, or a Mixed
///   value into a slot codegen leaves untouched): the store consumes (moves) its
///   value operand. An owning temporary is moved in as-is, but a *borrowed* value (a
///   parameter, local, or container read) must be `Acquire`d first. Without this,
///   storing a borrowed `Mixed` (e.g. `Class::$h = $handler` where `$handler` is a
///   `?SessionHandlerInterface` parameter) leaves the property dangling once the
///   borrow's owner releases its reference, so a later read dispatches on freed
///   memory (a fatal "on null").
pub(super) fn lower_static_property_assign(
    ctx: &mut LoweringContext<'_, '_>,
    receiver: &StaticReceiver,
    property: &str,
    value: &Expr,
    span: Span,
) {
    let value = lower_expr(ctx, value);
    let value = static_property_type(ctx, receiver, property)
        .map(|slot_ty| coerce_typed_assign_value(ctx, value, &slot_ty, span))
        .unwrap_or(value);
    if static_property_store_retains_independent_value(ctx, receiver, property, value) {
        store_static_property(ctx, receiver, property, value.value, span);
        if ctx.value_is_owning_temporary(value) {
            crate::ir_lower::ownership::release_if_owned(ctx, value, Some(span));
        }
        return;
    }
    let provisional_load = ctx.value_is_owned_unboxed_local_load(value.value);
    let stored = if ctx.value_is_owning_temporary(value) && !provisional_load {
        value
    } else {
        crate::ir_lower::ownership::acquire_if_refcounted(ctx, value, Some(span))
    };
    // A concrete local load can still be borrowed after final frame typing.
    // Retain its published owner, then retire only an actual Mixed unbox owner.
    if provisional_load {
        crate::ir_lower::ownership::release_if_owned(ctx, value, Some(span));
    }
    store_static_property(ctx, receiver, property, stored.value, span);
}

/// Returns true when codegen gives the static-property slot an independently retained value.
///
/// The rule is whether the store crosses the boxed/unboxed boundary. A concrete value going
/// into a Mixed/Union slot is boxed, and the box retains the child itself. A boxed value
/// going into a slot codegen narrows to -- `Str`, `Int`, `Bool`, `Float`, `Object`, or the
/// tagged-scalar pair -- is cast out of the box, and what lands in the slot owns nothing of
/// it (`__rt_str_persist` copies, the scalar casts read a payload word, the object arm
/// increfs the unboxed pointer on its own). Both directions leave the source's reference
/// unrelated to the slot's, so borrowed sources need no `Acquire` and owning temporary
/// sources must be released after the store -- without that release the box outlives its
/// last use and leaks, one cell per compound assignment (issue #1041).
///
/// Every other case is a moving store: a Mixed value into a Mixed/Union slot, or into one of
/// the container slots codegen leaves untouched, writes the pointer itself into the slot, so
/// the store consumes the reference. Unknown metadata conservatively keeps that discipline.
pub(super) fn static_property_store_retains_independent_value(
    ctx: &LoweringContext<'_, '_>,
    receiver: &StaticReceiver,
    property: &str,
    value: LoweredValue,
) -> bool {
    let Some(slot_ty) = static_property_type(ctx, receiver, property) else {
        return false;
    };
    let value_ty = ctx.builder.value_php_type(value.value);
    let slot_ty = slot_ty.codegen_repr();
    let value_ty = value_ty.codegen_repr();
    let value_is_boxed = matches!(value_ty, PhpType::Mixed | PhpType::Union(_));
    let boxes_into_mixed = matches!(slot_ty, PhpType::Mixed | PhpType::Union(_)) && !value_is_boxed;
    // The slot types codegen actually casts a Mixed source out of. Deliberately not "any slot
    // that is not Mixed": an array or iterable slot falls through `load_static_property_store_
    // value_to_result`'s catch-all arm untouched, so the Mixed pointer itself is what gets
    // stored and the store still consumes it.
    let narrows_out_of_mixed = value_is_boxed
        && matches!(
            slot_ty,
            PhpType::Str
                | PhpType::Int
                | PhpType::Bool
                | PhpType::Float
                | PhpType::Object(_)
                | PhpType::TaggedScalar
        );
    boxes_into_mixed || narrows_out_of_mixed
}

/// Lowers `Class::$prop[] = value`.
pub(super) fn lower_static_property_array_push(
    ctx: &mut LoweringContext<'_, '_>,
    receiver: &StaticReceiver,
    property: &str,
    value: &Expr,
    span: Span,
) {
    if let Some(array) = separate_php_array_static_property(ctx, receiver, property, span) {
        let value = lower_expr(ctx, value);
        ctx.emit_void(
            Op::MixedArrayAppend,
            vec![array.value, value.value],
            None,
            Op::MixedArrayAppend.default_effects(),
            Some(span),
        );
        if ctx.value_is_owning_temporary(value) {
            crate::ir_lower::ownership::release_if_owned(ctx, value, Some(span));
        }
        return;
    }
    if let Some(property_ty) =
        static_property_type(ctx, receiver, property).filter(is_indexed_array_type)
    {
        let property_value = load_static_property_as(ctx, receiver, property, property_ty, span);
        let value = lower_expr(ctx, value);
        ctx.emit_void(
            Op::ArrayPush,
            vec![property_value.value, value.value],
            None,
            Op::ArrayPush.default_effects(),
            Some(span),
        );
        store_static_property(ctx, receiver, property, property_value.value, span);
        return;
    }

    let property_value = load_static_property(ctx, receiver, property, span);
    let value = lower_expr(ctx, value);
    if static_property_may_be_eval_dynamic(ctx, receiver) {
        ctx.emit_void(
            Op::MixedArrayAppend,
            vec![property_value.value, value.value],
            None,
            Op::MixedArrayAppend.default_effects(),
            Some(span),
        );
        store_static_property(ctx, receiver, property, property_value.value, span);
        return;
    }
    let operands =
        crate::ir_lower::stmt::array_write_storage::object_append_operands(ctx, property_value, value);
    ctx.emit_void(
        Op::RuntimeCall,
        operands,
        None,
        effects_lookup::runtime_effects(),
        Some(span),
    );
}

/// Lowers `Class::$prop[index] = value`.
pub(super) fn lower_static_property_array_assign(
    ctx: &mut LoweringContext<'_, '_>,
    receiver: &StaticReceiver,
    property: &str,
    index: &Expr,
    value: &Expr,
    span: Span,
) {
    if let Some(array) = separate_php_array_static_property(ctx, receiver, property, span) {
        let (index, value) = array_write_core::lower_write_key_and_value(ctx, index, value);
        ctx.emit_void(
            Op::RuntimeCall,
            vec![array.value, index.value, value.value],
            None,
            effects_lookup::runtime_effects(),
            Some(span),
        );
        release_persisted_string_operand(ctx, index, span);
        if ctx.value_is_owning_temporary(value) {
            crate::ir_lower::ownership::release_if_owned(ctx, value, Some(span));
        }
        return;
    }
    if let Some(property_ty) =
        static_property_type(ctx, receiver, property).filter(is_indexed_array_type)
    {
        let array_ty = property_ty.clone();
        let property_value = load_static_property_as(ctx, receiver, property, property_ty, span);
        // PHP reads a plain-variable index at STORE time, after the right-hand side, so
        // `$o->a[$i] = ($i = 1)` writes index 1. The bare-local write already used this
        // rule; sharing the helper is what keeps the two from answering differently for
        // the same source line.
        let (index, value) =
            crate::ir_lower::stmt::array_write_core::lower_write_key_and_value(ctx, index, value);
        let index = coerce_array_key_to_int_at_span(ctx, index, Some(span), false);
        let value = coerce_indexed_array_set_value(ctx, &array_ty, value, Some(span));
        ctx.emit_void(
            Op::ArraySet,
            vec![property_value.value, index.value, value.value],
            None,
            Op::ArraySet.default_effects(),
            Some(span),
        );
        store_static_property(ctx, receiver, property, property_value.value, span);
        return;
    }

    let property_value = if let Some(property_ty) = static_property_type(ctx, receiver, property)
        .filter(|ty| type_satisfies_array_access_for_ir(ctx, ty))
    {
        load_static_property_as(ctx, receiver, property, property_ty, span)
    } else {
        load_static_property(ctx, receiver, property, span)
    };
    // PHP reads a plain-variable index at STORE time, after the right-hand side, so
    // `$o->a[$i] = ($i = 1)` writes index 1. The bare-local write already used this
    // rule; sharing the helper is what keeps the two from answering differently for
    // the same source line.
    let (index, value) =
        crate::ir_lower::stmt::array_write_core::lower_write_key_and_value(ctx, index, value);
    if static_property_may_be_eval_dynamic(ctx, receiver) {
        ctx.emit_void(
            Op::RuntimeCall,
            vec![property_value.value, index.value, value.value],
            None,
            effects_lookup::runtime_effects(),
            Some(span),
        );
        store_static_property(ctx, receiver, property, property_value.value, span);
        return;
    }
    ctx.emit_void(
        Op::RuntimeCall,
        vec![property_value.value, index.value, value.value],
        None,
        effects_lookup::runtime_effects(),
        Some(span),
    );
}

/// Publishes a detached boxed PHP array before a static-property element mutation.
/// The static slot consumes the new cell and releases its previous owner through ordinary storage.
fn separate_php_array_static_property(
    ctx: &mut LoweringContext<'_, '_>,
    receiver: &StaticReceiver,
    property: &str,
    span: Span,
) -> Option<LoweredValue> {
    let ty = static_property_type(ctx, receiver, property)?;
    if !ty.is_php_array() {
        return None;
    }
    let previous = load_static_property_as(ctx, receiver, property, ty.clone(), span);
    let separated = ctx.emit_value(
        Op::MixedClone,
        vec![previous.value],
        None,
        ty,
        Op::MixedClone.default_effects(),
        Some(span),
    );
    store_static_property(ctx, receiver, property, separated.value, span);
    Some(separated)
}

/// Returns true when a named static-property receiver may resolve through eval metadata.
pub(super) fn static_property_may_be_eval_dynamic(
    ctx: &LoweringContext<'_, '_>,
    receiver: &StaticReceiver,
) -> bool {
    let StaticReceiver::Named(class_name) = receiver else {
        return false;
    };
    ctx.has_eval_barrier()
        && !ctx
            .classes
            .contains_key(class_name.as_str().trim_start_matches('\\'))
}
