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
use crate::parser::ast::{CallableTarget, Expr, ExprKind};
use crate::types::FunctionSig;

use super::builtins::callable_lookup::{lookup_function, FunctionLookup};

/// Returns the capture list for a callable expression.
///
/// For closures and first-class callables, returns the captures stored in the
/// deferred closure context. For `$var` FCC variables, returns the captures
/// registered for that variable name. Otherwise returns an empty vector.
///
/// Each capture is a tuple of `(name, PhpType, is_mutable)` describing the
/// captured variable's name, PHP type, and whether it's mutated by the closure.
pub(crate) fn callable_captures(
    callback: &Expr,
    ctx: &mut Context,
) -> Vec<(String, crate::types::PhpType, bool)> {
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

/// Returns the FunctionSig for a callable expression.
///
/// Resolves string literals via `lookup_function` and checks user-defined functions
/// and include variants. Resolves `$var` variables from `ctx.closure_sigs`. Handles
/// first-class callables via `first_class_callable_sig`. For array-access expressions
/// where the array is a variable, resolves from `ctx.closure_sigs`. Delegates to
/// `matching_branch_sig` for ternary and null-coalescing branches that must share
/// the same signature. Returns `None` for expressions with no statically resolvable signature.
pub(crate) fn callable_sig(callback: &Expr, ctx: &Context) -> Option<FunctionSig> {
    match &callback.kind {
        ExprKind::StringLiteral(name) => match lookup_function(ctx, name) {
            Some(FunctionLookup::UserFunction(name))
            | Some(FunctionLookup::IncludeVariant(name)) => ctx.functions.get(&name).cloned(),
            _ => ctx.functions.get(name).cloned(),
        },
        ExprKind::Variable(name) => ctx.closure_sigs.get(name).cloned(),
        ExprKind::FirstClassCallable(target) => {
            crate::codegen::expr::calls::first_class_callable_sig(target, ctx)
        }
        ExprKind::FunctionCall { name, .. } => {
            let resolved_name = match lookup_function(ctx, name.as_str()) {
                Some(FunctionLookup::UserFunction(name))
                | Some(FunctionLookup::IncludeVariant(name)) => name,
                _ => name.as_str().to_string(),
            };
            ctx.callable_return_sigs.get(&resolved_name).cloned()
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

/// Computes the callable signature metadata for direct first class function.
pub(crate) fn direct_first_class_function_sig(
    callback: &Expr,
    ctx: &Context,
) -> Option<(String, FunctionSig)> {
    let target = match &callback.kind {
        ExprKind::FirstClassCallable(target) => Some(target),
        ExprKind::Variable(name) => ctx.first_class_callable_targets.get(name),
        _ => None,
    }?;
    let CallableTarget::Function(name) = target else {
        return None;
    };
    let resolved_name = match lookup_function(ctx, name.as_str())? {
        FunctionLookup::UserFunction(name) | FunctionLookup::IncludeVariant(name) => name,
        FunctionLookup::Builtin(_) | FunctionLookup::Extern(_) => return None,
    };
    let sig = ctx.functions.get(&resolved_name)?.clone();
    Some((resolved_name, sig))
}

/// Returns the common signature when both branches of a ternary or null-coalesce resolve to the same signature.
///
/// Recursively resolves signatures for the left and right expressions using `callable_sig`.
/// Returns the shared signature only if both branches resolve to an identical `FunctionSig`;
/// otherwise returns `None`. Used to determine whether a branch-shaped callable can be
/// emitted with a single code path.
fn matching_branch_sig(left: &Expr, right: &Expr, ctx: &Context) -> Option<FunctionSig> {
    let left_sig = callable_sig(left, ctx)?;
    let right_sig = callable_sig(right, ctx)?;
    if left_sig == right_sig {
        Some(left_sig)
    } else {
        None
    }
}
