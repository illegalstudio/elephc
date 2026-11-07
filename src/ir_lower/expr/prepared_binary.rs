//! Purpose:
//! Applies the shared PHP binary lowering to operands evaluated by a delayed write.
//!
//! Called from:
//! - Property dimension compound assignments after key, RHS and receiver selection.
//!
//! Key details:
//! - Prepared SSA operands preserve types without reevaluating either expression.
//! - The caller keeps original ownership and roots; ordinary binary semantics stay authoritative.

use super::*;

/// Reuses binary operator lowering with already evaluated, caller-owned operands.
pub(crate) fn lower_prepared_binary(
    ctx: &mut LoweringContext<'_, '_>, left: LoweredValue, op: &BinOp,
    right: LoweredValue, span: Span,
) -> LoweredValue {
    let left_expr = Expr::new(ExprKind::Variable("__eir_prepared_left".into()), span);
    let right_expr = Expr::new(ExprKind::Variable("__eir_prepared_right".into()), span);
    let expression = Expr::new(ExprKind::BinaryOp {
        left: Box::new(left_expr.clone()), op: op.clone(), right: Box::new(right_expr.clone()),
    }, span);
    if matches!(op, BinOp::Concat) {
        string_concat::lower_concat_from_values(ctx, left, right, &expression)
    } else {
        numeric_binary::lower_numeric_binary_from_values(ctx, left, op, right, &left_expr, &right_expr, &expression)
    }
}
