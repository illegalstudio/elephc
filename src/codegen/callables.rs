//! Purpose:
//! Shares callable metadata lookups used by indirect calls and callback builtins.
//! Centralizes capture and signature discovery so callable codegen paths stay aligned.
//!
//! Called from:
//! - `crate::codegen::expr::calls`
//! - `crate::codegen::builtins::arrays`
//!
//! Key details:
//! - Complex callable expressions can only expose captures when their runtime shape is statically direct.
//! - Branch-shaped callable signatures are reused only when every branch has the same call contract.

use crate::codegen::context::Context;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::{FunctionSig, PhpType};

pub(crate) fn callable_captures(callback: &Expr, ctx: &mut Context) -> Vec<(String, PhpType)> {
    match &callback.kind {
        ExprKind::Closure { .. } | ExprKind::FirstClassCallable(_) => ctx
            .deferred_closures
            .last()
            .map(|closure| closure.captures.clone())
            .unwrap_or_default(),
        ExprKind::Variable(name) => {
            ctx.mark_fcc_used(name);
            ctx.closure_captures.get(name).cloned().unwrap_or_default()
        }
        _ => Vec::new(),
    }
}

pub(crate) fn callable_sig(callback: &Expr, ctx: &Context) -> Option<FunctionSig> {
    match &callback.kind {
        ExprKind::StringLiteral(name) => ctx.functions.get(name).cloned(),
        ExprKind::Variable(name) => ctx.closure_sigs.get(name).cloned(),
        ExprKind::FirstClassCallable(target) => {
            crate::codegen::expr::calls::first_class_callable_sig(target, ctx)
        }
        ExprKind::ArrayAccess { array, .. } => {
            if let ExprKind::Variable(name) = &array.kind {
                ctx.closure_sigs.get(name).cloned()
            } else {
                None
            }
        }
        ExprKind::Assignment { value, .. } => callable_sig(value, ctx),
        ExprKind::Ternary {
            then_expr,
            else_expr,
            ..
        } => matching_branch_sig(then_expr, else_expr, ctx),
        ExprKind::ShortTernary { value, default }
        | ExprKind::NullCoalesce { value, default } => matching_branch_sig(value, default, ctx),
        _ => None,
    }
}

fn matching_branch_sig(left: &Expr, right: &Expr, ctx: &Context) -> Option<FunctionSig> {
    let left_sig = callable_sig(left, ctx)?;
    let right_sig = callable_sig(right, ctx)?;
    if left_sig == right_sig {
        Some(left_sig)
    } else {
        None
    }
}
