//! Purpose:
//! Removes native XML dimensions through opaque property receivers without array value copies.
//!
//! Called from:
//! - Direct unset lowering after known property and static storage paths.
//!
//! Key details:
//! - Computed bases and names run before the key; variable bases are read afterwards.
//! - False/null bases remain quiet, and unsupported ordinary storage raises an explicit Error.

use super::*;

/// Recognizes property paths whose erased base may carry native XML property semantics.
pub(super) fn supports(ctx: &LoweringContext<'_, '_>, array: &Expr) -> bool {
    let object = match &array.kind {
        ExprKind::PropertyAccess { object, property } if !property.starts_with("__elephc_") => object,
        ExprKind::DynamicPropertyAccess { object, .. } => object,
        _ => return false,
    };
    let ty = array_cast_source::array_cast_source_type_for_ir(ctx, object);
    // Non-null typed handles retain their existing path; nullable bases still need quiet fetches.
    if unset_array_access_has_object_receiver(ctx, array)
        && !matches!(ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_)) { return false; }
    ctx.classes.contains_key("SimpleXMLElement")
        && (matches!(ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_))
            || matches!(ty.codegen_repr(), PhpType::Object(class)
                if class.is_empty() || ctx.interfaces.contains_key(&class))
            || singular_object_class(&ty).is_some_and(|(class, _)|
                crate::types::builtin_classes::has_native_dimensions(class, ctx.classes)))
}

/// Keeps the native object branch independent of unsupported ordinary array storage places.
pub(super) fn lower(
    ctx: &mut LoweringContext<'_, '_>, array: &Expr, index: &Expr, expr: &Expr,
) {
    let (object, property) = match &array.kind {
        ExprKind::PropertyAccess { object, property } =>
            (object.as_ref(), Expr::new(ExprKind::StringLiteral(property.clone()), array.span)),
        ExprKind::DynamicPropertyAccess { object, property } =>
            (object.as_ref(), property.as_ref().clone()),
        _ => unreachable!("opaque property unset classification"),
    };
    let mut owners = Vec::new();
    let object = if matches!(object.kind, ExprKind::PropertyAccess { .. } | ExprKind::DynamicPropertyAccess { .. }) {
        // Intermediate properties are fetched for write; only the final property is a quiet probe.
        let value = property_access::lower_property_write_receiver(ctx, object);
        park_lowered_unset_operand(ctx, value, object.span, &mut owners)
    } else { park_unset_operand(ctx, object, &mut owners) };
    let property = park_unset_operand(ctx, &property, &mut owners);
    // A variable key must also be snapshotted before name conversion can replace its cell.
    let key = lower_expr(ctx, index);
    let key = park_lowered_unset_operand(ctx, key, index.span, &mut owners);
    let object = lower_expr(ctx, &object);
    let object = park_lowered_unset_operand(ctx, object, array.span, &mut owners);
    let object = lower_expr(ctx, &object);
    let class = ctx.intern_class_name("SimpleXMLElement");
    let native = ctx.emit_value(Op::InstanceOf, vec![object.value], Some(Immediate::Data(class)),
        PhpType::Bool, Op::InstanceOf.default_effects(), Some(expr.span));
    let yes = ctx.builder.create_named_block("unset.opaque.xml", Vec::new());
    let no = ctx.builder.create_named_block("unset.opaque.other", Vec::new());
    let merge = ctx.builder.create_named_block("unset.opaque.merge", Vec::new());
    let initial = ctx.initialized_slots_snapshot();
    ctx.builder.terminate(Terminator::CondBr { cond: native.value,
        then_target: yes, then_args: Vec::new(), else_target: no, else_args: Vec::new() });
    ctx.builder.position_at_end(yes);
    let receiver = borrow_native_xml_property_receiver(ctx, object, expr.span);
    remove_property_value(ctx, receiver, &property, &key, PropertyFetchMode::Probe,
        array, expr, &mut owners);
    jump(ctx, merge);
    let native_slots = ctx.initialized_slots_snapshot();
    ctx.builder.position_at_end(no);
    ctx.restore_initialized_slots(initial);
    let is_object = ctx.emit_value(Op::TypePredicate, vec![object.value],
        Some(Immediate::TypePredicate(crate::ir::PhpTypePredicate::Object)), PhpType::Bool,
        Op::TypePredicate.default_effects(), Some(expr.span));
    let ordinary = ctx.builder.create_named_block("unset.opaque.ordinary", Vec::new());
    ctx.builder.terminate(Terminator::CondBr { cond: is_object.value,
        then_target: ordinary, then_args: Vec::new(), else_target: merge, else_args: Vec::new() });
    let other_slots = ctx.initialized_slots_snapshot();
    ctx.builder.position_at_end(ordinary);
    remove_property_value(ctx, object, &property, &key, PropertyFetchMode::QuietRead,
        array, expr, &mut owners);
    jump(ctx, merge);
    let ordinary_slots = ctx.initialized_slots_snapshot();
    ctx.builder.position_at_end(merge);
    let other_slots = other_slots.intersection(&ordinary_slots).copied().collect();
    ctx.restore_initialized_slots(native_slots.intersection(&other_slots).copied().collect());
    for owner in owners.into_iter().rev() { retire_owned_call_operand(ctx, owner, expr.span); }
}

/// Removes through object handles, leaving unsupported array storage under an explicit Error.
fn remove_property_value(
    ctx: &mut LoweringContext<'_, '_>, receiver: LoweredValue, property: &Expr, key: &Expr,
    mode: PropertyFetchMode, array: &Expr, expr: &Expr, owners: &mut Vec<crate::ir::LocalSlotId>,
) {
    let inner_owners = owners.len();
    let name = lower_expr(ctx, property);
    let name = property_access::coerce_runtime_property_name(ctx, name, array.span);
    let name = park_lowered_unset_operand(ctx, name, array.span, owners);
    let name_value = lower_expr(ctx, &name);
    let selected = property_access::lower_dynamic_property_fetch_from_operands(
        ctx, receiver, name_value, PhpType::Mixed, mode, false, array,
    );
    let ty = ctx.builder.value_php_type(selected.value);
    let selected = if matches!(ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_)) { selected }
        else { ctx.box_value_as_mixed(selected, PhpType::Mixed, Some(expr.span)) };
    let selected = park_lowered_unset_operand(ctx, selected, array.span, owners);
    let selected = lower_expr(ctx, &selected);
    if mode == PropertyFetchMode::QuietRead {
        materialize_missing_null(ctx, receiver, name_value, selected, expr);
    }
    let is_array = ctx.emit_value(Op::TypePredicate, vec![selected.value],
        Some(Immediate::TypePredicate(crate::ir::PhpTypePredicate::Array)), PhpType::Bool,
        Op::TypePredicate.default_effects(), Some(expr.span));
    let unsupported = ctx.builder.create_named_block("unset.opaque.array", Vec::new());
    let other = ctx.builder.create_named_block("unset.opaque.value", Vec::new());
    let done = ctx.builder.create_named_block("unset.opaque.value.done", Vec::new());
    let initial = ctx.initialized_slots_snapshot();
    ctx.builder.terminate(Terminator::CondBr { cond: is_array.value,
        then_target: unsupported, then_args: Vec::new(), else_target: other, else_args: Vec::new() });
    ctx.builder.position_at_end(unsupported);
    crate::ir_lower::stmt::lower_throw_access_error(ctx,
        "Unsupported unset(): opaque array property storage has no attached removal path", expr.span);
    jump(ctx, done);
    ctx.builder.position_at_end(other);
    ctx.restore_initialized_slots(initial.clone());
    // OffsetUnset's object arm uses the native or public method table without copying an array.
    let is_object = ctx.emit_value(Op::TypePredicate, vec![selected.value],
        Some(Immediate::TypePredicate(crate::ir::PhpTypePredicate::Object)), PhpType::Bool,
        Op::TypePredicate.default_effects(), Some(expr.span));
    let object = ctx.builder.create_named_block("unset.opaque.object.value", Vec::new());
    let scalar = ctx.builder.create_named_block("unset.opaque.scalar.value", Vec::new());
    ctx.builder.terminate(Terminator::CondBr { cond: is_object.value,
        then_target: object, then_args: Vec::new(), else_target: scalar, else_args: Vec::new() });
    ctx.builder.position_at_end(object);
    let key = lower_expr(ctx, key);
    ctx.emit_void(Op::OffsetUnset, vec![selected.value, key.value], None,
        Op::OffsetUnset.default_effects(), Some(expr.span));
    jump(ctx, done);
    let object_slots = ctx.initialized_slots_snapshot();
    ctx.builder.position_at_end(scalar);
    ctx.restore_initialized_slots(initial.clone());
    mixed_dimension::lower_scalar(ctx, selected, expr.span);
    jump(ctx, done);
    let scalar_slots = ctx.initialized_slots_snapshot();
    ctx.builder.position_at_end(done);
    ctx.restore_initialized_slots(merge_initialized_slots_for_expr(&initial, object_slots, true, scalar_slots, true));
    for owner in owners.split_off(inner_owners).into_iter().rev() { retire_owned_call_operand(ctx, owner, expr.span); }
}

/// Preserves PHP's missing dynamic null slot without invoking a magic setter in place of unset.
fn materialize_missing_null(
    ctx: &mut LoweringContext<'_, '_>, receiver: LoweredValue, name: LoweredValue,
    selected: LoweredValue, expr: &Expr,
) {
    let done = ctx.builder.create_named_block("unset.opaque.null.done", Vec::new());
    let missing = ctx.builder.create_named_block("unset.opaque.null.lookup", Vec::new());
    let is_null = ctx.emit_value(Op::IsNull, vec![selected.value], None, PhpType::Bool,
        Op::IsNull.default_effects(), Some(expr.span));
    let initial = ctx.initialized_slots_snapshot();
    ctx.builder.terminate(Terminator::CondBr { cond: is_null.value,
        then_target: missing, then_args: Vec::new(), else_target: done, else_args: Vec::new() });
    ctx.builder.position_at_end(missing);
    let exists = metadata_bool(ctx, crate::ir::RuntimeFnId::PropertyExists, receiver, name, expr.span);
    let getter = ctx.builder.create_named_block("unset.opaque.null.getter", Vec::new());
    ctx.builder.terminate(Terminator::CondBr { cond: exists.value,
        then_target: done, then_args: Vec::new(), else_target: getter, else_args: Vec::new() });
    ctx.builder.position_at_end(getter);
    let get_name = lower_expr(ctx, &Expr::new(ExprKind::StringLiteral("__get".into()), expr.span));
    let has_get = metadata_bool(ctx, crate::ir::RuntimeFnId::MethodExists, receiver, get_name, expr.span);
    let setter = ctx.builder.create_named_block("unset.opaque.null.setter", Vec::new());
    ctx.builder.terminate(Terminator::CondBr { cond: has_get.value,
        then_target: done, then_args: Vec::new(), else_target: setter, else_args: Vec::new() });
    ctx.builder.position_at_end(setter);
    let set_name = lower_expr(ctx, &Expr::new(ExprKind::StringLiteral("__set".into()), expr.span));
    let has_set = metadata_bool(ctx, crate::ir::RuntimeFnId::MethodExists, receiver, set_name, expr.span);
    let unsupported = ctx.builder.create_named_block("unset.opaque.null.unsupported", Vec::new());
    let create = ctx.builder.create_named_block("unset.opaque.null.create", Vec::new());
    ctx.builder.terminate(Terminator::CondBr { cond: has_set.value,
        then_target: unsupported, then_args: Vec::new(), else_target: create, else_args: Vec::new() });
    ctx.builder.position_at_end(unsupported);
    crate::ir_lower::stmt::lower_throw_access_error(ctx,
        "Unsupported unset(): missing opaque property requires a raw storage fetch", expr.span);
    jump(ctx, done);
    ctx.builder.position_at_end(create);
    ctx.emit_void(Op::DynamicPropSet, vec![receiver.value, name.value, selected.value], None,
        Op::DynamicPropSet.default_effects(), Some(expr.span));
    jump(ctx, done);
    ctx.builder.position_at_end(done);
    ctx.restore_initialized_slots(initial);
}

/// Uses the shared typed metadata builtins without invoking user-named function aliases.
fn metadata_bool(
    ctx: &mut LoweringContext<'_, '_>, target: crate::ir::RuntimeFnId,
    receiver: LoweredValue, name: LoweredValue, span: Span,
) -> LoweredValue {
    ctx.emit_value(Op::RuntimeCall, vec![receiver.value, name.value],
        Some(Immediate::RuntimeCall(crate::ir::RuntimeCallTarget::Function(target))), PhpType::Bool, target.effects(), Some(span))
}

/// Joins an arm only when its preceding operation still has a normal continuation.
fn jump(ctx: &mut LoweringContext<'_, '_>, target: BlockId) {
    if !ctx.builder.insertion_block_is_terminated() {
        ctx.builder.terminate(Terminator::Br { target, args: Vec::new() });
    }
}
