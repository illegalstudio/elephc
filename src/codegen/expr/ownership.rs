//! Purpose:
//! Classifies expression results as owned, borrowed, persistent, or non-refcounted for cleanup decisions.
//! Provides retain and release helpers used around heap-valued temporaries and arguments.
//!
//! Called from:
//! - `crate::codegen::expr` and statement cleanup paths
//!
//! Key details:
//! - Ownership answers must stay conservative to avoid leaks, double frees, and borrowed-value releases.

use crate::codegen::context::HeapOwnership;
use crate::parser::ast::{Expr, ExprKind};

pub(crate) fn expr_result_heap_ownership(expr: &Expr) -> HeapOwnership {
    match &expr.kind {
        ExprKind::Variable(_)
        | ExprKind::ArrayAccess { .. }
        | ExprKind::PropertyAccess { .. }
        | ExprKind::NullsafePropertyAccess { .. }
        | ExprKind::StaticPropertyAccess { .. }
        | ExprKind::This => HeapOwnership::Borrowed,
        ExprKind::Spread(inner)
        | ExprKind::PtrCast { expr: inner, .. }
        | ExprKind::Cast { expr: inner, .. } => expr_result_heap_ownership(inner),
        ExprKind::Print(_) => HeapOwnership::NonHeap,
        ExprKind::Throw(_) => HeapOwnership::NonHeap,
        ExprKind::NullCoalesce { value, default } => {
            expr_result_heap_ownership(value).merge(expr_result_heap_ownership(default))
        }
        ExprKind::Ternary {
            then_expr,
            else_expr,
            ..
        } => expr_result_heap_ownership(then_expr).merge(expr_result_heap_ownership(else_expr)),
        ExprKind::ShortTernary { value, default } => {
            expr_result_heap_ownership(value).merge(expr_result_heap_ownership(default))
        }
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
        | ExprKind::NullsafeMethodCall { .. }
        | ExprKind::StaticMethodCall { .. }
        | ExprKind::NewObject { .. } => HeapOwnership::Owned,
        _ => HeapOwnership::NonHeap,
    }
}
