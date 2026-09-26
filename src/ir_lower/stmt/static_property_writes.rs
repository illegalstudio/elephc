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
    let source = lower_expr(ctx, value);
    let source = static_property_type(ctx, receiver, property)
        .map(|slot_ty| coerce_typed_assign_value(ctx, source, &slot_ty, span))
        .unwrap_or(source);
    if static_property_store_retains_independent_value(ctx, receiver, property, source) {
        store_static_property(ctx, receiver, property, source.value, span);
        if ctx.value_is_owning_temporary(source) {
            crate::ir_lower::ownership::release_if_owned(ctx, source, Some(span));
        }
        return;
    }
    // A `Str` is PERSISTED into the slot whatever its ownership says, and the source is then
    // released. Ownership is the wrong question for a string here: "owning" distinguishes who
    // must release, not WHERE the bytes live, and a great many string producers hand back a
    // pointer into the shared 64 KiB `_concat_buf` scratch. Moving such a pointer into a slot
    // that outlives the statement stores the right LENGTH over bytes the next producer
    // overwrites — `B::$s = strtoupper($x)` read back a later `str_repeat`'s bytes.
    //
    // Fixing this at the producer instead (reclassifying runtime-call `Str` results as
    // non-owning) was tried and is wrong twice over: it only reaches the producers someone
    // thought to list — `$x . "c"`, `"v=$x!"`, `(string)$i`, `strval($i)` all still clobbered —
    // and above 64 KiB `__rt_concat_reserve` returns an OWNED heap block, so suppressing the
    // release leaked one block per call. The consumer is the only place that knows the value is
    // about to outlive the frame, so it is the only place the question can be answered once.
    //
    // The three storage classes all land correctly, which is why this is safe rather than
    // merely conservative:
    // - scratch slice: `Acquire` is `__rt_str_persist`, which duplicates it onto the heap; the
    //   paired `Release` reaches `__rt_heap_free_safe`, which skips a non-heap pointer.
    // - owned heap block (a unary-string result over 64 KiB): duplicated, then the original is
    //   freed — this is the leak the producer-side attempt introduced.
    // - `CONCAT_TEMP_HEAP_KIND` block (a `.` result over 64 KiB): `__rt_str_persist` takes it
    //   over IN PLACE, and the release cannot double-free it because codegen classifies
    //   `Op::StrConcat` as a scratch string and drops its `Release` entirely.
    //
    // `store_local` reached the same answer for `static` LOCALS at `context.rs`'s
    // `static_local_store_needs_string_retain`; a static property has the identical lifetime
    // problem and now has the identical rule.
    let stores_a_string = matches!(
        ctx.builder.value_php_type(source.value).codegen_repr(),
        PhpType::Str
    );
    let source_is_owning_temporary = ctx.value_is_owning_temporary(source);
    let provisional_load = ctx.value_is_owned_unboxed_local_load(source.value);
    let stored = if stores_a_string || !source_is_owning_temporary || provisional_load {
        crate::ir_lower::ownership::acquire_if_refcounted(ctx, source, Some(span))
    } else {
        source
    };
    // A concrete local load can still be borrowed after final frame typing.
    // Retain its published owner, then retire only an actual Mixed unbox owner.
    if provisional_load {
        crate::ir_lower::ownership::release_if_owned(ctx, source, Some(span));
    }
    store_static_property(ctx, receiver, property, stored.value, span);
    // The string rule's own release, SKIPPED when the provisional-load arm already issued
    // one: the two conditions overlap for an owned unboxed local load of a `Str`, and
    // releasing the same source twice is a double free rather than a tidy-up.
    if stores_a_string && source_is_owning_temporary && !provisional_load {
        crate::ir_lower::ownership::release_if_owned(ctx, source, Some(span));
    }
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
    ctx.emit_void(
        Op::RuntimeCall,
        vec![property_value.value, value.value],
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
