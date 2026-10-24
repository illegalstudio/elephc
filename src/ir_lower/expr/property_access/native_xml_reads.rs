//! Purpose:
//! Preserves fresh SimpleXML property selections behind boxed or union receivers.
//!
//! Called from:
//! - Named property-read lowering when the XML capability is present.
//!
//! Key details:
//! - Ordinary objects keep the existing read path; XML views own their public property names.
//! - The native getter remains an owned EIR call instead of an opaque borrowed property read.

use super::*;

/// Selects the native getter by runtime type, then joins both owned result paths.
pub(super) fn lower(
    ctx: &mut LoweringContext<'_, '_>, object: LoweredValue, property: &str, op: Op, expr: &Expr,
) -> LoweredValue {
    let result_staging = prepublish_call_result(ctx, &PhpType::Mixed, expr.span);
    let (object, owner) = root_owned_call_operand(ctx, object, expr.span);
    let class = ctx.intern_class_name("SimpleXMLElement");
    let condition = ctx.emit_value(Op::InstanceOf, vec![object.value], Some(Immediate::Data(class)),
        PhpType::Bool, Op::InstanceOf.default_effects(), Some(expr.span));
    let temp = ctx.declare_owned_hidden_temp(PhpType::Mixed);
    let initial = ctx.initialized_slots_snapshot();
    let native = ctx.builder.create_named_block("xml.property", vec![]);
    let regular = ctx.builder.create_named_block("xml.property.fallback", vec![]);
    let merge = ctx.builder.create_named_block("xml.property.merge", vec![]);
    ctx.builder.terminate(Terminator::CondBr { cond: condition.value,
        then_target: native, then_args: vec![], else_target: regular, else_args: vec![] });
    ctx.builder.position_at_end(native);
    let receiver = borrow_receiver(ctx, object, expr.span);
    let name = Expr::new(ExprKind::StringLiteral(property.into()), expr.span);
    let value = lower_method_call_with_receiver(ctx, receiver, "__get", &[name], Op::MethodCall, expr);
    store_value_into_temp(ctx, &temp, PhpType::Mixed, value, expr.span);
    branch_to(ctx, merge);
    let native_initialized = ctx.initialized_slots_snapshot();
    ctx.builder.position_at_end(regular);
    ctx.restore_initialized_slots(initial.clone());
    let receiver = borrow_receiver(ctx, object, expr.span);
    let value = lower_property_get_without_native_dispatch(ctx, receiver, property, op, expr);
    store_value_into_temp(ctx, &temp, PhpType::Mixed, value, expr.span);
    branch_to(ctx, merge);
    let regular_initialized = ctx.initialized_slots_snapshot();
    ctx.builder.position_at_end(merge);
    ctx.restore_initialized_slots(merge_initialized_slots_for_expr(&initial,
        native_initialized, true, regular_initialized, true));
    let result = take_owned_temp(ctx, &temp, expr.span);
    stage_call_result(ctx, result_staging.as_ref(), result, expr.span);
    if let Some(owner) = owner { retire_owned_call_operand(ctx, owner, expr.span); }
    take_prepublished_call_result(ctx, result_staging, result, expr.span)
}

/// Lends the boxed receiver to either branch without consuming its common owner.
fn borrow_receiver(ctx: &mut LoweringContext<'_, '_>, object: LoweredValue, span: Span) -> LoweredValue {
    let ty = ctx.builder.value_php_type(object.value);
    let value = ctx.emit_value(Op::Borrow, vec![object.value], None, ty,
        Op::Borrow.default_effects(), Some(span));
    ctx.builder.set_value_ownership(value.value, Ownership::Borrowed);
    value
}
