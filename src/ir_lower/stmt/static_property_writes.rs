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
/// - **Boxing store** (a Mixed/Union slot receiving a non-Mixed value, e.g.
///   `Class::$h = new C()`): codegen boxes the value with `__rt_mixed_from_value`,
///   which takes its *own* retained reference to the child. The slot therefore
///   keeps a reference independent of the source, so an owning temporary must be
///   *released* after the store (its reference is not the one the slot holds), and
///   a borrowed source must be left untouched. Acquiring here would leak the extra
///   reference on top of the box's retained one.
/// - **Moving store** (every other case: concrete-typed slot, or a Mixed→Mixed
///   move): the store consumes (moves) its value operand. An owning temporary is
///   moved in as-is, but a *borrowed* value (a parameter, local, or container read)
///   must be `Acquire`d first. Without this, storing a borrowed `Mixed`
///   (e.g. `Class::$h = $handler` where `$handler` is a `?SessionHandlerInterface`
///   parameter) leaves the property dangling once the borrow's owner releases its
///   reference, so a later read dispatches on freed memory (a fatal "on null").
pub(super) fn lower_static_property_assign(
    ctx: &mut LoweringContext<'_, '_>,
    receiver: &StaticReceiver,
    property: &str,
    value: &Expr,
    span: Span,
) {
    let value = lower_expr(ctx, value);
    if static_property_store_retains_independent_value(ctx, receiver, property, value) {
        store_static_property(ctx, receiver, property, value.value, span);
        if ctx.value_is_owning_temporary(value) {
            crate::ir_lower::ownership::release_if_owned(ctx, value, Some(span));
        }
        return;
    }
    let value = if ctx.value_is_owning_temporary(value) {
        value
    } else {
        crate::ir_lower::ownership::acquire_if_refcounted(ctx, value, Some(span))
    };
    store_static_property(ctx, receiver, property, value.value, span);
}

/// Returns true when codegen gives the static-property slot an independently retained value.
///
/// This covers both concrete values boxed into Mixed/Union slots and boxed Mixed values
/// unboxed into object slots. Both backend paths retain the stored child independently,
/// so borrowed sources need no `Acquire` and owning temporary sources are released after
/// the store. Unknown metadata conservatively keeps the moving-store discipline.
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
    let boxes_into_mixed = matches!(slot_ty, PhpType::Mixed | PhpType::Union(_))
        && !matches!(value_ty, PhpType::Mixed | PhpType::Union(_));
    let unboxes_into_object = matches!(slot_ty, PhpType::Object(_))
        && matches!(value_ty, PhpType::Mixed | PhpType::Union(_));
    boxes_into_mixed || unboxes_into_object
}

/// Lowers `Class::$prop[] = value`.
pub(super) fn lower_static_property_array_push(
    ctx: &mut LoweringContext<'_, '_>,
    receiver: &StaticReceiver,
    property: &str,
    value: &Expr,
    span: Span,
) {
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

    // HASH storage, the shape a declared `array` takes on once a string key is written into it.
    // Without this the write fell through to the Mixed fallback below while every READ of the
    // same property loaded it as a hash — `self::$store[$k] = $v` then `count(self::$store)`
    // answered 0, in silence. The instance-property path has had this branch all along; this is
    // the same one, through the static accessors.
    if let Some(property_ty) =
        static_property_type(ctx, receiver, property).filter(is_assoc_array_type)
    {
        let property_value = load_static_property_as(ctx, receiver, property, property_ty, span);
        // PHP reads a plain-variable index at STORE time, after the right-hand side, so
        // `$o->a[$i] = ($i = 1)` writes index 1.
        let (index, value) =
            crate::ir_lower::stmt::array_write_core::lower_write_key_and_value(ctx, index, value);
        ctx.emit_void(
            Op::HashSet,
            vec![property_value.value, index.value, value.value],
            None,
            Op::HashSet.default_effects(),
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

