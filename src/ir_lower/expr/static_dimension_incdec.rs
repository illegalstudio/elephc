//! Purpose:
//! Preserves native XML selection for indexed static incdec expressions.
//!
//! Called from:
//! - Assignment-expression lowering before the ordinary parser capture replay.
//!
//! Key details:
//! - Recognizes canonical parser captures without changing ordinary array updates.
//! - Native updates return the scalar or old view without storing it into the node.
//! - Results and receivers stay rooted through warnings and operator callbacks.

use super::*;

/// Routes canonical static indexed incdec through native selection or its ordinary capture replay.
pub(super) fn lower(
    ctx: &mut LoweringContext<'_, '_>, target: &Expr, value: &Expr, result: Option<&Expr>,
    prelude: &[Stmt], conditional: Option<&str>, expr: &Expr,
    fallback: impl FnOnce(&mut LoweringContext<'_, '_>, &[Stmt]) -> LoweredValue,
) -> Option<LoweredValue> {
    if conditional.is_some() { return None; }
    let ExprKind::ArrayAccess { array, index } = &target.kind else { return None; };
    if !matches!(array.kind, ExprKind::StaticPropertyAccess { .. }) { return None; }
    let ty = array_cast_source::array_cast_source_type_for_ir(ctx, array);
    if !property_access::may_have_native_xml_dimensions(ctx, &ty) { return None; }
    let ExprKind::Variable(old) = &value.kind else { return None; };
    if !crate::names::is_generated_local_name(old) || !old.contains("__elephc_incdec_old_") { return None; }
    let (operation, earlier) = prelude.split_last()?;
    let StmtKind::Assign { name: next, value: operation } = &operation.kind else { return None; };
    if !result.is_some_and(|result| matches!(&result.kind, ExprKind::Variable(name) if name == next)) {
        return None;
    }
    let (increment, post, operand) = match &operation.kind {
        ExprKind::PreIncrement(name) => (true, false, name),
        ExprKind::PostIncrement(name) => (true, true, name),
        ExprKind::PreDecrement(name) => (false, false, name),
        ExprKind::PostDecrement(name) => (false, true, name),
        _ => return None,
    };
    if operand != old { return None; }
    let (read, captures) = earlier.split_last()?;
    if !matches!(&read.kind, StmtKind::Assign { name, value } if name == old && value == target) {
        return None;
    }
    for statement in captures { crate::ir_lower::stmt::lower_stmt(ctx, statement); }
    let array = lower_expr(ctx, array);
    Some(property_access::native_xml_reads::dispatch(
        ctx, array, expr, PhpType::Mixed, false,
        |ctx, receiver| native(ctx, receiver, index, increment, post, expr),
        |ctx, _| {
            let result = fallback(ctx, &prelude[captures.len()..]);
            let result = ctx.box_value_as_mixed(result, PhpType::Mixed, Some(expr.span));
            crate::ir_lower::ownership::copy_assignment_value(ctx, result, Some(expr.span))
        },
    ))
}

/// Retains the selected XML receiver and transfers the incdec result before working-slot cleanup.
fn native(
    ctx: &mut LoweringContext<'_, '_>, receiver: LoweredValue, index: &Expr,
    increment: bool, post: bool, expr: &Expr,
) -> LoweredValue {
    let staging = prepublish_call_result(ctx, &PhpType::Mixed, expr.span);
    let ty = ctx.builder.value_php_type(receiver.value);
    let borrowed = ctx.emit_value(Op::Borrow, vec![receiver.value], None, ty.clone(),
        Op::Borrow.default_effects(), Some(expr.span));
    ctx.builder.set_value_ownership(borrowed.value, Ownership::Borrowed);
    let owned = if matches!(ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_)) {
        ctx.emit_owned_value(Op::MixedClone, vec![borrowed.value], None, PhpType::Mixed,
            Op::MixedClone.default_effects(), Some(expr.span))
    } else { crate::ir_lower::ownership::acquire_lifetime_pin_if_refcounted(ctx, borrowed, Some(expr.span)) };
    let lease = crate::ir_lower::stmt::property_write_receiver::PropertyWriteReceiver::new(ctx, owned, expr.span);
    let receiver = borrow_native_xml_property_receiver(ctx, lease.value, expr.span);
    let key = lower_expr(ctx, index);
    let selected = lower_native_dimension_incdec_select_from_values(ctx, receiver, key, !post, expr);
    let mut result = None;
    crate::ir_lower::stmt::with_dimension_incdec_value(ctx, selected, increment, post, expr.span,
        |ctx, value| {
            let value = ctx.box_value_as_mixed(value, PhpType::Mixed, Some(expr.span));
            let value = crate::ir_lower::ownership::copy_assignment_value(ctx, value, Some(expr.span));
            stage_call_result(ctx, staging.as_ref(), value, expr.span);
            result = Some(value);
        });
    let result = result.expect("incdec always publishes its expression result");
    lease.finish(ctx, expr.span);
    take_prepublished_call_result(ctx, staging, result, expr.span)
}
