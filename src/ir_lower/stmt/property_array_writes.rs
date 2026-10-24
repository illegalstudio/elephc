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
    let receiver = Expr::new(ExprKind::PropertyAccess {
        object: Box::new(object.clone()), property: property.into(),
    }, span);
    if crate::ir_lower::expr::lower_weak_map_denied_append(ctx, &receiver, value, span) { return; }
    let mut roots = Vec::new();
    let (mut receiver, value) = if crate::ir_lower::expr::is_static_property_write_chain(object) {
        let value = lower_rooted_property_write_operand(ctx, value, &mut roots);
        let receiver = crate::ir_lower::expr::lower_static_property_write_chain(ctx, object, value, span);
        (receiver, value)
    } else {
        let object = lower_expr(ctx, object);
        let mut receiver = property_write_receiver::PropertyWriteReceiver::new(ctx, object, span);
        let value = lower_rooted_property_write_operand(ctx, value, &mut roots);
        receiver.place_above_operand_roots(ctx, &roots, span);
        (receiver, value)
    };
    receiver.narrow_for_array(ctx, property, &[value.value], span);
    lower_property_array_push_value(ctx, receiver.value, property, value, span);
    receiver.finish(ctx, span);
    retire_property_write_roots(ctx, roots, span);
}

/// Mutates the array property while the caller holds its receiver lease.
fn lower_property_array_push_value(
    ctx: &mut LoweringContext<'_, '_>, object: LoweredValue, property: &str, value: LoweredValue, span: Span,
) {
    if object_property_type(ctx, object.value, property).is_some_and(|ty| ty.is_php_array()) {
        lower_php_array_property_write(ctx, object, property, None, value, span, false);
        return;
    }
    if let Some(property_ty) =
        object_property_type(ctx, object.value, property).filter(is_indexed_array_type)
    {
        let value = ctx.retype_value_for_container_element(&property_ty, value, Some(span));
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
    if matches!(&update, Some(ElementUpdate::Compound | ElementUpdate::IncDec { .. }))
        && crate::ir_lower::expr::lower_weak_map_incdec_stmt(ctx, value, span)
    {
        return;
    }
    if let Some(ElementUpdate::NullCoalesce { read, default }) = update {
        crate::ir_lower::expr::lower_null_coalesce_update_stmt(ctx, read, default, span);
        return;
    }
    let element_update = match &update {
        Some(ElementUpdate::IncDec { increment }) => PropertyElementUpdate::IncDec(*increment),
        _ => PropertyElementUpdate::Written,
    };
    lower_property_array_assign_with_update(
        ctx,
        object,
        property,
        index,
        value,
        span,
        update.is_some(),
        element_update,
    );
}

/// How a property-array element write may store a typed scalar buffer update.
#[derive(Clone, Copy)]
pub(super) enum PropertyElementUpdate {
    /// The value is lowered as written.
    Written,
    /// A statement `++`/`--` (`true` for `++`), typed once the receiver shows a scalar buffer.
    IncDec(bool),
    /// An expression `++`/`--` whose element read and step already ran, as the parser's prelude
    /// runs them before the write: the write stores this value instead of lowering `value`.
    Stored(LoweredValue),
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
    lower_property_array_assign_with_update(
        ctx,
        object,
        property,
        index,
        value,
        span,
        key_already_diagnosed,
        PropertyElementUpdate::Written,
    );
}

/// Lowers `$object->prop[index] = value`, storing a typed buffer update when `update` names one.
///
/// A statement `++`/`--` is decided after the receiver is lowered, from the same property type
/// that selects the `BufferSet` write below, and its element read then runs where `value` would.
/// `value` is not lowered when the write stores a typed update.
pub(super) fn lower_property_array_assign_with_update(
    ctx: &mut LoweringContext<'_, '_>,
    object: &Expr,
    property: &str,
    index: &Expr,
    value: &Expr,
    span: Span,
    key_already_diagnosed: bool,
    update: PropertyElementUpdate,
) {
    let mut roots = Vec::new();
    let (mut receiver, index, value) = if crate::ir_lower::expr::is_static_property_write_chain(object) {
        let source = match update {
            PropertyElementUpdate::Stored(stored) => PropertyWriteValue::Stored(stored),
            PropertyElementUpdate::Written | PropertyElementUpdate::IncDec(_) => {
                PropertyWriteValue::Expr(value)
            }
        };
        let (index, value) = lower_property_write_operands(ctx, index, source, &mut roots, span);
        let receiver = crate::ir_lower::expr::lower_static_property_write_chain_with_operands(
            ctx, object, &[index.value, value.value], span);
        (receiver, index, value)
    } else {
        let object = lower_expr(ctx, object);
        let mut receiver = property_write_receiver::PropertyWriteReceiver::new(ctx, object, span);
        let source = match update {
            PropertyElementUpdate::Written => PropertyWriteValue::Expr(value),
            PropertyElementUpdate::IncDec(increment) => {
                super::buffer_element_updates::property_statement_update(
                    ctx, receiver.value, property, value, increment,
                )
                .map_or(PropertyWriteValue::Expr(value), PropertyWriteValue::Buffer)
            }
            PropertyElementUpdate::Stored(stored) => PropertyWriteValue::Stored(stored),
        };
        let (index, value) = lower_property_write_operands(ctx, index, source, &mut roots, span);
        receiver.place_above_operand_roots(ctx, &roots, span);
        (receiver, index, value)
    };
    receiver.narrow_for_array(ctx, property, &[index.value, value.value], span);
    lower_property_array_assign_value(ctx, receiver.value, property, index, value, span, key_already_diagnosed);
    receiver.finish(ctx, span);
    retire_property_write_roots(ctx, roots, span);
}

/// The value a property-array write stores, resolved once the receiver is known.
enum PropertyWriteValue<'a> {
    /// The written expression, lowered and rooted as an ordinary operand.
    Expr(&'a Expr),
    /// A statement `++`/`--` of a scalar buffer element, read and stepped where `Expr` would run.
    Buffer(super::buffer_element_updates::BufferIncDec<'a>),
    /// A typed update the caller already read and stepped.
    Stored(LoweredValue),
}

/// Lowers a property-array write's key and stored value in PHP's variable-key order.
fn lower_property_write_operands(
    ctx: &mut LoweringContext<'_, '_>,
    index: &Expr,
    source: PropertyWriteValue<'_>,
    roots: &mut Vec<LocalSlotId>,
    span: Span,
) -> (LoweredValue, LoweredValue) {
    match source {
        PropertyWriteValue::Expr(value) => {
            lower_rooted_property_write_key_and_value(ctx, index, value, roots)
        }
        // The typed element values are scalars, so they need no unwind root.
        PropertyWriteValue::Buffer(buffer) => lower_rooted_property_write_key_then(
            ctx, index, roots, |ctx, _| buffer.lower_values(ctx, span).stored,
        ),
        PropertyWriteValue::Stored(stored) => {
            lower_rooted_property_write_key_then(ctx, index, roots, |_, _| stored)
        }
    }
}

/// Lowers one operand into an unwind-visible owner and lends a borrowed view to the writer.
fn lower_rooted_property_write_operand(
    ctx: &mut LoweringContext<'_, '_>,
    expr: &Expr,
    roots: &mut Vec<LocalSlotId>,
) -> LoweredValue {
    let value = lower_expr(ctx, expr);
    let (value, root) = crate::ir_lower::expr::root_owned_call_operand(ctx, value, expr.span);
    let Some(root) = root else {
        return value;
    };
    roots.push(root);
    let ty = ctx.builder.value_php_type(value.value);
    let borrowed = ctx.emit_value(
        Op::Borrow,
        vec![value.value],
        None,
        ty,
        Op::Borrow.default_effects(),
        Some(expr.span),
    );
    ctx.builder.set_value_ownership(borrowed.value, Ownership::Borrowed);
    borrowed
}

/// Preserves PHP's variable-key timing while rooting every evaluated write operand immediately.
fn lower_rooted_property_write_key_and_value(
    ctx: &mut LoweringContext<'_, '_>,
    index: &Expr,
    value: &Expr,
    roots: &mut Vec<LocalSlotId>,
) -> (LoweredValue, LoweredValue) {
    lower_rooted_property_write_key_then(ctx, index, roots, |ctx, roots| {
        lower_rooted_property_write_operand(ctx, value, roots)
    })
}

/// Roots the key of a property-array write and runs `lower_value` in PHP's variable-key order.
///
/// A bare variable key is read after the value and a computed key before it, exactly as for
/// `lower_rooted_property_write_key_and_value`; `lower_value` receives the same root list.
fn lower_rooted_property_write_key_then<'m, 'f, T>(
    ctx: &mut LoweringContext<'m, 'f>,
    index: &Expr,
    roots: &mut Vec<LocalSlotId>,
    lower_value: impl FnOnce(&mut LoweringContext<'m, 'f>, &mut Vec<LocalSlotId>) -> T,
) -> (LoweredValue, T) {
    if matches!(index.kind, ExprKind::Variable(_)) {
        let value = lower_value(ctx, roots);
        return (lower_rooted_property_write_operand(ctx, index, roots), value);
    }
    let index = lower_rooted_property_write_operand(ctx, index, roots);
    let value = lower_value(ctx, roots);
    (index, value)
}

/// Releases rooted operands after the receiver lease and write have both completed.
fn retire_property_write_roots(
    ctx: &mut LoweringContext<'_, '_>, roots: Vec<LocalSlotId>, span: Span,
) {
    for root in roots.into_iter().rev() {
        crate::ir_lower::expr::retire_owned_call_operand(ctx, root, span);
    }
}

/// Mutates the indexed property while the caller holds its receiver lease.
fn lower_property_array_assign_value(
    ctx: &mut LoweringContext<'_, '_>, object: LoweredValue, property: &str, index: LoweredValue,
    value: LoweredValue, span: Span, key_already_diagnosed: bool,
) {
    if object_property_type(ctx, object.value, property).is_some_and(|ty| ty.is_php_array()) {
        lower_php_array_property_write(
            ctx,
            object,
            property,
            Some(index),
            value,
            span,
            key_already_diagnosed,
        );
        return;
    }
    if let Some(property_ty) = object_property_type(ctx, object.value, property)
        .filter(|ty| matches!(ty.codegen_repr(), PhpType::Buffer(_)))
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
        let index = coerce_to_int_at_span(ctx, index, Some(span));
        let value = coerce_buffer_set_value(ctx, &property_ty, value, Some(span));
        ctx.emit_void(
            Op::BufferSet,
            vec![property_value.value, index.value, value.value],
            None,
            Op::BufferSet.default_effects(),
            Some(span),
        );
        crate::ir_lower::ownership::release_if_owned(ctx, property_value, Some(span));
        return;
    }
    if let Some(property_ty) =
        object_property_type(ctx, object.value, property).filter(is_indexed_array_type)
    {
        let value = ctx.retype_value_for_container_element(&property_ty, value, Some(span));
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
        let value = ctx.retype_value_for_container_element(&property_ty, value, Some(span));
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
        // Object offsetSet borrows its operands just like a local boxed writer.
        // Reuse its evaluation order and unwind roots for fresh keys and values.
        array_write_core::lower_boxed_array_set_values(
            ctx,
            property_value,
            index,
            value,
            span,
            false,
        );
        return;
    }

    // PHP reads a plain-variable index at STORE time, after the right-hand side, so
    // `$o->a[$i] = ($i = 1)` writes index 1. The bare-local write already used this
    // rule; sharing the helper is what keeps the two from answering differently for
    // the same source line.
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
fn lower_php_array_property_write(
    ctx: &mut LoweringContext<'_, '_>,
    object: LoweredValue,
    property: &str,
    index: Option<LoweredValue>,
    value: LoweredValue,
    span: Span,
    key_already_diagnosed: bool,
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
    if let Some(index) = index {
        ctx.emit_void(
            Op::RuntimeCall,
            vec![array.value, index.value, value.value],
            key_already_diagnosed.then_some(Immediate::Bool(true)),
            effects_lookup::runtime_effects(),
            Some(span),
        );
        release_persisted_string_operand(ctx, index, span);
    } else {
        ctx.emit_void(
            Op::MixedArrayAppend,
            vec![array.value, value.value],
            None,
            Op::MixedArrayAppend.default_effects(),
            Some(span),
        );
    }
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
    // Mixed slots retain when EIR owns source cleanup, including a release on a readonly
    // error sibling. Keep a normal-path release and an unwind pin instead of assuming adoption.
    if matches!(value_ty, PhpType::Mixed | PhpType::Union(_))
        && matches!(property_ty, PhpType::Int | PhpType::Bool | PhpType::Float | PhpType::TaggedScalar)
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
