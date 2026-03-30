use crate::codegen::context::HeapOwnership;
use crate::parser::ast::{Expr, ExprKind};

pub(super) fn expr_result_heap_ownership(expr: &Expr) -> HeapOwnership {
    match &expr.kind {
        ExprKind::Variable(_)
        | ExprKind::ArrayAccess { .. }
        | ExprKind::PropertyAccess { .. }
        | ExprKind::This => HeapOwnership::Borrowed,
        ExprKind::Spread(inner)
        | ExprKind::PtrCast { expr: inner, .. }
        | ExprKind::Cast { expr: inner, .. } => expr_result_heap_ownership(inner),
        ExprKind::Throw(_) => HeapOwnership::NonHeap,
        ExprKind::NullCoalesce { value, default } => {
            expr_result_heap_ownership(value).merge(expr_result_heap_ownership(default))
        }
        ExprKind::Ternary {
            then_expr,
            else_expr,
            ..
        } => expr_result_heap_ownership(then_expr).merge(expr_result_heap_ownership(else_expr)),
        ExprKind::Match { arms, default, .. } => {
            let mut ownership = default
                .as_ref()
                .map(|expr| expr_result_heap_ownership(expr))
                .unwrap_or(HeapOwnership::NonHeap);
            for (_, expr) in arms {
                ownership = ownership.merge(expr_result_heap_ownership(expr));
            }
            ownership
        }
        ExprKind::StringLiteral(_)
        | ExprKind::ArrayLiteral(_)
        | ExprKind::ArrayLiteralAssoc(_)
        | ExprKind::FunctionCall { .. }
        | ExprKind::ClosureCall { .. }
        | ExprKind::ExprCall { .. }
        | ExprKind::MethodCall { .. }
        | ExprKind::StaticMethodCall { .. }
        | ExprKind::NewObject { .. } => HeapOwnership::Owned,
        _ => HeapOwnership::NonHeap,
    }
}
