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
    ctx.classes.contains_key("SimpleXMLElement")
        && (matches!(ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_))
            || matches!(ty.codegen_repr(), PhpType::Object(class)
                if class.is_empty() || ctx.interfaces.contains_key(&class)))
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
    let object = park_unset_operand(ctx, object, &mut owners);
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
    let native_owners = owners.len();
    let receiver = borrow_native_xml_property_receiver(ctx, object, expr.span);
    let selected = property_access::lower_dynamic_property_fetch_from_value(
        ctx, receiver, &property, PropertyFetchMode::Probe, array,
    );
    let selected = park_lowered_unset_operand(ctx, selected, array.span, &mut owners);
    let selected = lower_expr(ctx, &selected);
    let ty = ctx.builder.value_php_type(selected.value);
    native_dimension_probes::lower(ctx, selected, &ty, &key,
        elephc_builtin_contract::ObjectDimension::Unset, expr);
    for owner in owners.split_off(native_owners).into_iter().rev() {
        retire_owned_call_operand(ctx, owner, expr.span);
    }
    jump(ctx, merge);
    let native_slots = ctx.initialized_slots_snapshot();
    ctx.builder.position_at_end(no);
    ctx.restore_initialized_slots(initial);
    let is_object = ctx.emit_value(Op::TypePredicate, vec![object.value],
        Some(Immediate::TypePredicate(crate::ir::PhpTypePredicate::Object)), PhpType::Bool,
        Op::TypePredicate.default_effects(), Some(expr.span));
    let unsupported = ctx.builder.create_named_block("unset.opaque.unsupported", Vec::new());
    ctx.builder.terminate(Terminator::CondBr { cond: is_object.value,
        then_target: unsupported, then_args: Vec::new(), else_target: merge, else_args: Vec::new() });
    let other_slots = ctx.initialized_slots_snapshot();
    ctx.builder.position_at_end(unsupported);
    crate::ir_lower::stmt::lower_throw_access_error(ctx,
        "Unsupported unset(): opaque ordinary object property storage has no attached removal path", expr.span);
    jump(ctx, merge);
    ctx.builder.position_at_end(merge);
    ctx.restore_initialized_slots(native_slots.intersection(&other_slots).copied().collect());
    for owner in owners.into_iter().rev() { retire_owned_call_operand(ctx, owner, expr.span); }
}

/// Joins an arm only when its preceding operation still has a normal continuation.
fn jump(ctx: &mut LoweringContext<'_, '_>, target: BlockId) {
    if !ctx.builder.insertion_block_is_terminated() {
        ctx.builder.terminate(Terminator::Br { target, args: Vec::new() });
    }
}
