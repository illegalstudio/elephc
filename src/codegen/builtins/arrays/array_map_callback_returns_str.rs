use crate::codegen::context::Context;
use crate::parser::ast::{Expr, ExprKind, StmtKind};
use crate::types::PhpType;

use super::array_map_expr_is_str::expr_is_str;

/// Infer whether a callback returns a string type from its AST.
pub(super) fn callback_returns_str(args: &[Expr], ctx: &Context) -> bool {
    match &args[0].kind {
        ExprKind::Closure { body, .. } => {
            for stmt in body {
                if let StmtKind::Return(Some(expr)) = &stmt.kind {
                    return expr_is_str(expr);
                }
            }
            false
        }
        ExprKind::StringLiteral(name) => {
            if let Some(sig) = ctx.functions.get(name) {
                return sig.return_type == PhpType::Str;
            }
            false
        }
        _ => false,
    }
}
