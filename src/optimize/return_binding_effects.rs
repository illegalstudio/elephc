//! Purpose:
//! Models implicit declared string return verification before AST pruning.
//!
//! Called from:
//! - Callable effect summaries, worker-entry projections and exact exception summaries.
//!
//! Key details:
//! - Source conversions execute before return verification and keep their own effects.
//! - Unknown returned values may invoke Stringable code and fail with TypeError.
//! - Simple string literals, concatenation results and untouched string parameters need no extra binding.

use super::*;

/// Adds conservative string-return verification effects beyond the source expression's own effects.
pub(super) fn effect(
    params: &[(String, Option<TypeExpr>, Option<Expr>, bool)],
    return_type: &Option<TypeExpr>, body: &[Stmt],
) -> Effect {
    if !matches!(return_type, Some(TypeExpr::Str)) { return Effect::PURE; }
    if let [Stmt { kind: StmtKind::Return(Some(expr)), .. }] = body {
        match &expr.kind {
            ExprKind::StringLiteral(_) | ExprKind::BinaryOp { op: BinOp::Concat, .. } => return Effect::PURE,
            ExprKind::Variable(name) if params.iter().any(|(parameter, ty, _, by_ref)| {
                parameter == name && !by_ref && matches!(ty, Some(TypeExpr::Str))
            }) => return Effect::PURE,
            ExprKind::IntLiteral(_) | ExprKind::FloatLiteral(_) | ExprKind::BoolLiteral(_) | ExprKind::Null => {
                return Effect::PURE.with_may_throw();
            }
            _ => {}
        }
    }
    effect_analysis::string_binding_effect()
}

/// Reports whether implicit declared-string return verification can throw for this callable body.
pub(super) fn may_throw(
    params: &[(String, Option<TypeExpr>, Option<Expr>, bool)],
    return_type: &Option<TypeExpr>, body: &[Stmt],
) -> bool {
    effect(params, return_type, body).may_throw
}
