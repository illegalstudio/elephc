//! Purpose:
//! Instance property array mutations and retaining-store cleanup.
//!
//! Called from:
//! - `crate::ir_lower::stmt`.
//!
//! Key details:
//! - Preserves statement ordering, CFG shape, EIR effects, and ownership contracts.

use super::*;

/// Lowers `$object->prop[] = value`.
pub(super) fn lower_property_array_push(
    ctx: &mut LoweringContext<'_, '_>,
    object: &Expr,
    property: &str,
    value: &Expr,
    span: Span,
) {
    let object = lower_expr(ctx, object);
    // A value that may reassign the property is lowered, and pinned, before the property is
    // fetched, so the append lands in the array the property holds afterwards (see
    // `element_write_order`).
    let mut slots = Vec::new();
    let prelowered = super::element_write_order::property_element_operands_may_write_property(
        property,
        &[value],
    )
    .then(|| {
        let lowered = lower_expr(ctx, value);
        let (pinned, slot) = crate::ir_lower::expr::root_call_operand(ctx, lowered, span);
        slots.extend(slot);
        pinned
    });
    lower_property_array_push_into(ctx, object, property, value, span, prelowered);
    for slot in slots.into_iter().rev() {
        crate::ir_lower::expr::retire_owned_call_operand(ctx, slot, span);
    }
}

/// Lowers `$object->prop[] = value` into an already lowered receiver object. `prelowered` is the
/// value when the caller lowered it before the property fetch.
fn lower_property_array_push_into(
    ctx: &mut LoweringContext<'_, '_>,
    object: LoweredValue,
    property: &str,
    value: &Expr,
    span: Span,
    prelowered: Option<LoweredValue>,
) {
    if object_property_type(ctx, object.value, property).is_some_and(|ty| ty.is_php_array()) {
        let prelowered = prelowered.map(|value| (None, value));
        lower_php_array_property_write(ctx, object, property, None, value, span, false, prelowered);
        return;
    }
    if let Some(property_ty) =
        object_property_type(ctx, object.value, property).filter(is_indexed_array_type)
    {
        let data = ctx.intern_string(property);
        let property_value = ctx.emit_value(
            Op::PropGet,
            vec![object.value],
            Some(Immediate::Data(data)),
            property_ty.clone(),
            Op::PropGet.default_effects(),
            Some(span),
        );
        let property_value =
            crate::ir_lower::ownership::acquire_if_refcounted(ctx, property_value, Some(span));
        let value = prelowered.unwrap_or_else(|| lower_expr(ctx, value));
        ctx.emit_void(
            Op::ArrayPush,
            vec![property_value.value, value.value],
            None,
            Op::ArrayPush.default_effects(),
            Some(span),
        );
        release_property_array_insert_value_after_retain(ctx, &property_ty, value, span);
        ctx.emit_void(
            Op::PropSet,
            vec![object.value, property_value.value],
            Some(Immediate::Data(data)),
            Op::PropSet.default_effects(),
            Some(span),
        );
        release_rewritten_property_value_after_retaining_store(
            ctx,
            &property_ty,
            property_value,
            span,
        );
        return;
    }

    let value = prelowered.unwrap_or_else(|| lower_expr(ctx, value));
    let data = ctx.intern_string(property);
    ctx.emit_void(
        Op::RuntimeCall,
        vec![object.value, value.value],
        Some(Immediate::Data(data)),
        effects_lookup::runtime_effects(),
        Some(span),
    );
}

/// Lowers `$object->prop[index] = value`.
///
/// A desugared `??=` becomes a probe plus a conditional insert, and a desugared compound update
/// writes with the key its own read already converted (see `ElementUpdate`).
pub(super) fn lower_property_array_assign(
    ctx: &mut LoweringContext<'_, '_>,
    object: &Expr,
    property: &str,
    index: &Expr,
    value: &Expr,
    span: Span,
) {
    let update = desugared_element_update(value, span, |read| {
        reads_property_array_element(read, object, property, index)
    });
    if let Some(ElementUpdate::NullCoalesce { read, default }) = update {
        crate::ir_lower::expr::lower_null_coalesce_update_stmt(ctx, read, default, span);
        return;
    }
    lower_property_array_assign_with_diagnosed_key(
        ctx,
        object,
        property,
        index,
        value,
        span,
        update.is_some(),
    );
}

/// Returns whether `read` is the element `$object->property[index]` a statement writes.
fn reads_property_array_element(read: &Expr, object: &Expr, property: &str, index: &Expr) -> bool {
    matches!(
        &read.kind,
        ExprKind::ArrayAccess { array, index: read_index }
            if read_index.as_ref() == index
                && matches!(
                    &array.kind,
                    ExprKind::PropertyAccess { object: read_object, property: read_property }
                        if read_object.as_ref() == object && read_property == property
                )
    )
}

/// Lowers `$object->prop[index] = value`, optionally reusing a float-key diagnosis.
///
/// `key_already_diagnosed` marks the write half of a compound update: its read of the same
/// element already reported the float key's conversion, as PHP does once per update. An
/// `ArrayAccess` property receives the key unconverted, and a runtime-typed receiver keeps its
/// own conversion, so neither consults the flag.
pub(crate) fn lower_property_array_assign_with_diagnosed_key(
    ctx: &mut LoweringContext<'_, '_>,
    object: &Expr,
    property: &str,
    index: &Expr,
    value: &Expr,
    span: Span,
    key_already_diagnosed: bool,
) {
    let object = lower_expr(ctx, object);
    // A key or value that may reassign the property is lowered, and pinned, before the property
    // is fetched, so the write lands in the array the property holds afterwards (see
    // `element_write_order`).
    let mut slots = Vec::new();
    let prelowered = super::element_write_order::property_element_operands_may_write_property(
        property,
        &[index, value],
    )
    .then(|| {
        let (index, value, pinned) =
            crate::ir_lower::stmt::array_write_core::lower_pinned_write_key_and_value(
                ctx, index, value, span,
            );
        slots = pinned;
        (index, value)
    });
    lower_property_array_assign_into(
        ctx, object, property, index, value, span, key_already_diagnosed, prelowered,
    );
    for slot in slots.into_iter().rev() {
        crate::ir_lower::expr::retire_owned_call_operand(ctx, slot, span);
    }
}

/// Lowers `$object->prop[index] = value` into an already lowered receiver object. `prelowered`
/// is the key and value when the caller lowered them before the property fetch.
#[allow(clippy::too_many_arguments)]
fn lower_property_array_assign_into(
    ctx: &mut LoweringContext<'_, '_>,
    object: LoweredValue,
    property: &str,
    index: &Expr,
    value: &Expr,
    span: Span,
    key_already_diagnosed: bool,
    prelowered: Option<(LoweredValue, LoweredValue)>,
) {
    if object_property_type(ctx, object.value, property).is_some_and(|ty| ty.is_php_array()) {
        let prelowered = prelowered.map(|(index, value)| (Some(index), value));
        lower_php_array_property_write(
            ctx,
            object,
            property,
            Some(index),
            value,
            span,
            key_already_diagnosed,
            prelowered,
        );
        return;
    }
    if let Some(property_ty) =
        object_property_type(ctx, object.value, property).filter(is_indexed_array_type)
    {
        let data = ctx.intern_string(property);
        let property_value = ctx.emit_value(
            Op::PropGet,
            vec![object.value],
            Some(Immediate::Data(data)),
            property_ty.clone(),
            Op::PropGet.default_effects(),
            Some(span),
        );
        let property_value =
            crate::ir_lower::ownership::acquire_if_refcounted(ctx, property_value, Some(span));
        // PHP reads a plain-variable index at STORE time, after the right-hand side, so
        // `$o->a[$i] = ($i = 1)` writes index 1. The bare-local write already used this
        // rule; sharing the helper is what keeps the two from answering differently for
        // the same source line.
        let (index, value) = prelowered.unwrap_or_else(|| {
            crate::ir_lower::stmt::array_write_core::lower_write_key_and_value(ctx, index, value)
        });
        let index =
            coerce_array_key_to_int_at_span(ctx, index, Some(span), key_already_diagnosed);
        let value = coerce_indexed_array_set_value(ctx, &property_ty, value, Some(span));
        ctx.emit_void(
            Op::ArraySet,
            vec![property_value.value, index.value, value.value],
            None,
            Op::ArraySet.default_effects(),
            Some(span),
        );
        release_property_array_insert_value_after_retain(ctx, &property_ty, value, span);
        ctx.emit_void(
            Op::PropSet,
            vec![object.value, property_value.value],
            Some(Immediate::Data(data)),
            Op::PropSet.default_effects(),
            Some(span),
        );
        release_rewritten_property_value_after_retaining_store(
            ctx,
            &property_ty,
            property_value,
            span,
        );
        return;
    }
    if let Some(property_ty) =
        object_property_type(ctx, object.value, property).filter(is_assoc_array_type)
    {
        let data = ctx.intern_string(property);
        let property_value = ctx.emit_value(
            Op::PropGet,
            vec![object.value],
            Some(Immediate::Data(data)),
            property_ty.clone(),
            Op::PropGet.default_effects(),
            Some(span),
        );
        let property_value =
            crate::ir_lower::ownership::acquire_if_refcounted(ctx, property_value, Some(span));
        // PHP reads a plain-variable index at STORE time, after the right-hand side, so
        // `$o->a[$i] = ($i = 1)` writes index 1. The bare-local write already used this
        // rule; sharing the helper is what keeps the two from answering differently for
        // the same source line.
        let (index, value) = prelowered.unwrap_or_else(|| {
            crate::ir_lower::stmt::array_write_core::lower_write_key_and_value(ctx, index, value)
        });
        ctx.emit_void(
            Op::HashSet,
            vec![property_value.value, index.value, value.value],
            key_already_diagnosed.then_some(Immediate::Bool(true)),
            Op::HashSet.default_effects(),
            Some(span),
        );
        release_property_array_insert_value_after_retain(ctx, &property_ty, value, span);
        ctx.emit_void(
            Op::PropSet,
            vec![object.value, property_value.value],
            Some(Immediate::Data(data)),
            Op::PropSet.default_effects(),
            Some(span),
        );
        release_rewritten_property_value_after_retaining_store(
            ctx,
            &property_ty,
            property_value,
            span,
        );
        return;
    }

    if let Some(property_ty) = object_property_type(ctx, object.value, property)
        .filter(|ty| type_satisfies_array_access_for_ir(ctx, ty))
    {
        let data = ctx.intern_string(property);
        let property_value = ctx.emit_value(
            Op::PropGet,
            vec![object.value],
            Some(Immediate::Data(data)),
            property_ty,
            Op::PropGet.default_effects(),
            Some(span),
        );
        // PHP reads a plain-variable index at STORE time, after the right-hand side, so
        // `$o->a[$i] = ($i = 1)` writes index 1. The bare-local write already used this
        // rule; sharing the helper is what keeps the two from answering differently for
        // the same source line.
        let (index, value) = prelowered.unwrap_or_else(|| {
            crate::ir_lower::stmt::array_write_core::lower_write_key_and_value(ctx, index, value)
        });
        ctx.emit_void(
            Op::RuntimeCall,
            vec![property_value.value, index.value, value.value],
            None,
            effects_lookup::runtime_effects(),
            Some(span),
        );
        return;
    }

    // PHP reads a plain-variable index at STORE time, after the right-hand side, so
    // `$o->a[$i] = ($i = 1)` writes index 1. The bare-local write already used this
    // rule; sharing the helper is what keeps the two from answering differently for
    // the same source line.
    let (index, value) = prelowered.unwrap_or_else(|| {
        crate::ir_lower::stmt::array_write_core::lower_write_key_and_value(ctx, index, value)
    });
    let data = ctx.intern_string(property);
    ctx.emit_void(
        Op::RuntimeCall,
        vec![object.value, index.value, value.value],
        Some(Immediate::Data(data)),
        effects_lookup::runtime_effects(),
        Some(span),
    );
}

/// Separates a declared PHP array property before mutating its packed-or-hash boxed payload.
/// `PropGetForWrite` publishes the detached cell and returns a borrow owned by the property.
/// `key_already_diagnosed` lets the boxed writer rebuild a float key its read already reported.
/// `prelowered` carries a key and value the caller lowered BEFORE the property fetch, because
/// they may reassign the property (see `element_write_order`).
#[allow(clippy::too_many_arguments)]
fn lower_php_array_property_write(
    ctx: &mut LoweringContext<'_, '_>,
    object: LoweredValue,
    property: &str,
    index: Option<&Expr>,
    value: &Expr,
    span: Span,
    key_already_diagnosed: bool,
    prelowered: Option<(Option<LoweredValue>, LoweredValue)>,
) {
    let data = ctx.intern_string(property);
    let array = ctx.emit_value(
        Op::PropGetForWrite,
        vec![object.value],
        Some(Immediate::Data(data)),
        PhpType::php_array(),
        Op::PropGetForWrite.default_effects(),
        Some(span),
    );
    ctx.builder.set_value_ownership(array.value, Ownership::Borrowed);
    let value = if let Some(index) = index {
        let (index, value) = match prelowered {
            Some((Some(index), value)) => (index, value),
            _ => array_write_core::lower_write_key_and_value(ctx, index, value),
        };
        ctx.emit_void(
            Op::RuntimeCall,
            vec![array.value, index.value, value.value],
            key_already_diagnosed.then_some(Immediate::Bool(true)),
            effects_lookup::runtime_effects(),
            Some(span),
        );
        release_persisted_string_operand(ctx, index, span);
        value
    } else {
        let value = match prelowered {
            Some((_, value)) => value,
            None => lower_expr(ctx, value),
        };
        ctx.emit_void(
            Op::MixedArrayAppend,
            vec![array.value, value.value],
            None,
            Op::MixedArrayAppend.default_effects(),
            Some(span),
        );
        value
    };
    if ctx.value_is_owning_temporary(value) {
        crate::ir_lower::ownership::release_if_owned(ctx, value, Some(span));
    }
}

/// Releases a temporary assigned into an object property after `PropSet` retains or boxes it.
pub(in crate::ir_lower) fn release_property_assignment_source_after_retaining_store(
    ctx: &mut LoweringContext<'_, '_>,
    property_ty: &PhpType,
    value: LoweredValue,
    span: Span,
) {
    if !ctx.value_is_owning_temporary(value) {
        return;
    }
    if !property_store_keeps_independent_ref(property_ty, &ctx.builder.value_php_type(value.value))
    {
        return;
    }
    crate::ir_lower::ownership::release_if_owned(ctx, value, Some(span));
}

/// Releases an element temporary after a property-array write retains it for storage.
pub(super) fn release_property_array_insert_value_after_retain(
    ctx: &mut LoweringContext<'_, '_>,
    property_ty: &PhpType,
    value: LoweredValue,
    span: Span,
) {
    if !property_array_insert_retains_value(ctx, property_ty, value) {
        return;
    }
    if ctx.value_is_owning_temporary(value) {
        crate::ir_lower::ownership::release_if_owned(ctx, value, Some(span));
    }
}

/// Returns true when the backend insert helper takes its own reference to the element value.
///
/// Three disciplines meet here, and only the first needs a release:
///
/// - **Indexed storage** (`__rt_array_set_*`, `__rt_array_push_*`): the value is incref'd into
///   the slot unconditionally, so an owning temporary keeps a reference nobody consumes.
/// - **Mixed-element indexed storage from a concrete value**: codegen boxes the value first,
///   and the boxing path releases the box it made, so releasing here would double-free.
/// - **Assoc storage** (`__rt_hash_set`): the value is only incref'd when it cannot hand over
///   its own reference (`value_can_own_mixed_box_source`), so an owning temporary is moved in
///   and the store consumes it.
///
/// A Mixed-element *indexed* array fed an already-boxed Mixed value therefore falls in the
/// first group, not the second: nothing boxes, the incref still fires, and without the release
/// every `$o->items[0] += 1` / `$o->items[] = f()` leaks one Mixed cell (issue #1041).
fn property_array_insert_retains_value(
    ctx: &LoweringContext<'_, '_>,
    property_ty: &PhpType,
    value: LoweredValue,
) -> bool {
    let Some(elem_ty) = indexed_property_array_element_type(property_ty) else {
        return false;
    };
    if !matches!(elem_ty.codegen_repr(), PhpType::Mixed | PhpType::Callable) {
        return true;
    }
    if !matches!(property_ty.codegen_repr(), PhpType::Array(_)) {
        return false;
    }
    matches!(
        ctx.builder.value_php_type(value.value).codegen_repr(),
        PhpType::Mixed | PhpType::Union(_)
    )
}

/// Releases the loaded property value after rewriting it through a retaining `PropSet`.
pub(super) fn release_rewritten_property_value_after_retaining_store(
    ctx: &mut LoweringContext<'_, '_>,
    property_ty: &PhpType,
    property_value: LoweredValue,
    span: Span,
) {
    if property_ty.codegen_repr().is_refcounted() {
        crate::ir_lower::ownership::release_if_owned(ctx, property_value, Some(span));
    }
}

/// Returns whether a property store creates a distinct retained/boxed owner for the value.
pub(super) fn property_store_keeps_independent_ref(property_ty: &PhpType, value_ty: &PhpType) -> bool {
    let property_ty = property_ty.codegen_repr();
    let value_ty = value_ty.codegen_repr();
    if matches!((&property_ty, &value_ty), (PhpType::Mixed, PhpType::Mixed)) {
        return false;
    }
    if matches!(value_ty, PhpType::Mixed | PhpType::Union(_))
        && matches!(property_ty, PhpType::Int | PhpType::Bool | PhpType::Float)
    {
        return true;
    }
    // Callable slots retain through the descriptor ABI, outside is_refcounted().
    if matches!(property_ty, PhpType::Str | PhpType::Callable) {
        return true;
    }
    property_ty.is_refcounted()
}

/// Returns the element type for property arrays that use retaining indexed/hash helpers.
pub(super) fn indexed_property_array_element_type(property_ty: &PhpType) -> Option<PhpType> {
    match property_ty.codegen_repr() {
        PhpType::Array(elem_ty) => Some(elem_ty.codegen_repr()),
        PhpType::AssocArray { value, .. } => Some(value.codegen_repr()),
        _ => None,
    }
}
