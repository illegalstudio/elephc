//! Purpose:
//! Reuses numeric operator lowering for already evaluated builtin operands.
//!
//! Called from:
//! - The full BuiltinLoweringContext adapter for numeric graph builtins.
//!
//! Key details:
//! - Argument expressions have already run in their physical source mode.
//! - Explicit Borrow views protect caller leases while fresh operator intermediates keep ownership.
//! - The invocation contract applies only to the operator graph, including GMP dispatch.

use super::*;

/// Applies the builtin invocation policy after source-order evaluation has produced both operands.
pub(in crate::ir_lower) fn lower(
    ctx: &mut LoweringContext<'_, '_>, left: ValueId, operation: BinOp, right: ValueId,
    source_mode: crate::source::SourceMode, span: Span,
) -> LoweredValue {
    let previous = ctx.source_mode;
    ctx.source_mode = source_mode;
    let result = ctx.with_independent_write_receiver(|ctx| {
        let left = borrowed_operand(ctx, left, span);
        let right = borrowed_operand(ctx, right, span);
        // Unknown syntactic operands preserve integer power selection instead of suggesting a
        // floating literal result. The real types remain attached to the evaluated SSA values.
        let left_expr = Expr::new(ExprKind::Variable("__builtin_numeric_left".into()), span);
        let right_expr = Expr::new(ExprKind::Variable("__builtin_numeric_right".into()), span);
        let expression = Expr::new(ExprKind::BinaryOp {
            left: Box::new(left_expr.clone()), op: operation.clone(), right: Box::new(right_expr.clone()),
        }, span);
        lower_numeric_binary_from_values(ctx, left, &operation, right, &left_expr, &right_expr, &expression)
    });
    ctx.source_mode = previous;
    result
}

/// Borrows the original argument without suppressing ownership of newly created numeric boxes.
pub(in crate::ir_lower) fn borrowed_operand(ctx: &mut LoweringContext<'_, '_>, operand: ValueId, span: Span) -> LoweredValue {
    let ty = ctx.builder.value_php_type(operand);
    let value = LoweredValue { value: operand, ir_type: ctx.builder.value_type(operand) };
    if !Ownership::php_type_needs_lifetime_tracking(&ty) { return value; }
    let borrowed = ctx.emit_value(Op::Borrow, vec![operand], None, ty,
        Op::Borrow.default_effects(), Some(span));
    ctx.builder.set_value_ownership(borrowed.value, Ownership::Borrowed);
    borrowed
}
