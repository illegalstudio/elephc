//! Purpose:
//! Shares callable metadata lookups used by indirect calls and callback builtins.
//! Centralizes capture and signature discovery so callable codegen paths stay aligned.
//!
//! Called from:
//! - `crate::codegen_support::expr::calls`
//! - `crate::codegen_support::builtins::arrays`
//!
//! Key details:
//! - Complex callable expressions can only expose captures when their runtime shape is statically direct.
//! - Branch-shaped callable signatures are reused only when every branch has the same call contract.

use crate::codegen_support::context::Context;
use crate::names::php_symbol_key;
use crate::parser::ast::{CallableTarget, Expr, ExprKind, StaticReceiver};
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
            crate::codegen_support::expr::calls::first_class_callable_sig(target, ctx)
        }
        ExprKind::FunctionCall { name, .. } => {
            let resolved_name = match lookup_function(ctx, name.as_str()) {
                Some(FunctionLookup::UserFunction(name))
                | Some(FunctionLookup::IncludeVariant(name)) => name,
                _ => name.as_str().to_string(),
            };
            ctx.callable_return_sigs.get(&resolved_name).cloned()
        }
        ExprKind::MethodCall { object, method, .. } => {
            method_return_callable_sig(object, method, ctx, false)
        }
        ExprKind::StaticMethodCall {
            receiver, method, ..
        } => static_method_return_callable_sig(receiver, method, ctx, false),
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

/// Returns the element callable signature for an expression that yields an array of callables.
///
/// This is kept separate from `callable_sig()` because a function returning
/// `array<callable>` is not itself callable, but callers that later read an element
/// from the returned array still need the element descriptor signature.
pub(crate) fn callable_array_sig(callback_array: &Expr, ctx: &Context) -> Option<FunctionSig> {
    match &callback_array.kind {
        ExprKind::ArrayLiteral(elems) => matching_array_element_sig(elems.iter(), ctx),
        ExprKind::ArrayLiteralAssoc(entries) => {
            matching_array_element_sig(entries.iter().map(|(_, value)| value), ctx)
        }
        ExprKind::FunctionCall { name, .. } => {
            let resolved_name = match lookup_function(ctx, name.as_str()) {
                Some(FunctionLookup::UserFunction(name))
                | Some(FunctionLookup::IncludeVariant(name)) => name,
                _ => name.as_str().to_string(),
            };
            ctx.callable_array_return_sigs.get(&resolved_name).cloned()
        }
        ExprKind::MethodCall { object, method, .. } => {
            method_return_callable_sig(object, method, ctx, true)
        }
        ExprKind::StaticMethodCall {
            receiver, method, ..
        } => static_method_return_callable_sig(receiver, method, ctx, true),
        ExprKind::Variable(name) => ctx.closure_sigs.get(name).cloned(),
        ExprKind::Assignment { value, .. } => callable_array_sig(value, ctx),
        ExprKind::Ternary {
            then_expr,
            else_expr,
            ..
        } => matching_array_branch_sig(then_expr, else_expr, ctx),
        ExprKind::ShortTernary { value, default }
        | ExprKind::NullCoalesce { value, default } => {
            matching_array_branch_sig(value, default, ctx)
        }
        _ => None,
    }
}

/// Returns a shared callable signature only when every array element has the same contract.
fn matching_array_element_sig<'a>(
    values: impl Iterator<Item = &'a Expr>,
    ctx: &Context,
) -> Option<FunctionSig> {
    let mut shared_sig: Option<FunctionSig> = None;
    let mut saw_value = false;
    for value in values {
        saw_value = true;
        let sig = callable_sig(value, ctx)?;
        match &shared_sig {
            Some(existing) if existing != &sig => return None,
            Some(_) => {}
            None => shared_sig = Some(sig),
        }
    }
    if saw_value {
        shared_sig
    } else {
        None
    }
}

/// Returns a callable-array signature only when both branches share one element contract.
fn matching_array_branch_sig(left: &Expr, right: &Expr, ctx: &Context) -> Option<FunctionSig> {
    let left_sig = callable_array_sig(left, ctx)?;
    let right_sig = callable_array_sig(right, ctx)?;
    if left_sig == right_sig {
        Some(left_sig)
    } else {
        None
    }
}

/// Resolves callable-return metadata for an instance method call expression.
fn method_return_callable_sig(
    object: &Expr,
    method: &str,
    ctx: &Context,
    array_return: bool,
) -> Option<FunctionSig> {
    let object_ty = crate::codegen_support::functions::infer_contextual_type(object, ctx);
    let class_name = crate::codegen_support::functions::singular_object_class(&object_ty)?.to_string();
    let method_key = php_symbol_key(method);
    let impl_class = ctx
        .classes
        .get(&class_name)
        .and_then(|class_info| class_info.method_impl_classes.get(&method_key))
        .cloned()
        .unwrap_or(class_name);
    stored_method_return_callable_sig(&impl_class, &method_key, ctx, array_return)
}

/// Resolves callable-return metadata for a static method call expression.
fn static_method_return_callable_sig(
    receiver: &StaticReceiver,
    method: &str,
    ctx: &Context,
    array_return: bool,
) -> Option<FunctionSig> {
    let class_name = resolve_static_method_metadata_class(receiver, ctx)?;
    let method_key = php_symbol_key(method);
    let class_info = ctx.classes.get(&class_name)?;
    let impl_class = class_info
        .static_method_impl_classes
        .get(&method_key)
        .or_else(|| class_info.method_impl_classes.get(&method_key))
        .cloned()
        .unwrap_or(class_name);
    stored_method_return_callable_sig(&impl_class, &method_key, ctx, array_return)
}

/// Looks up stored callable-return metadata for a class method.
fn stored_method_return_callable_sig(
    class_name: &str,
    method_key: &str,
    ctx: &Context,
    array_return: bool,
) -> Option<FunctionSig> {
    let class_info = ctx.classes.get(class_name)?;
    if array_return {
        class_info
            .callable_array_method_return_sigs
            .get(method_key)
            .cloned()
    } else {
        class_info.callable_method_return_sigs.get(method_key).cloned()
    }
}

/// Resolves the concrete class whose static method metadata should be inspected.
fn resolve_static_method_metadata_class(
    receiver: &StaticReceiver,
    ctx: &Context,
) -> Option<String> {
    match receiver {
        StaticReceiver::Named(name) => resolve_class_name(ctx, name.as_str()).map(str::to_string),
        StaticReceiver::Self_ | StaticReceiver::Static => ctx.current_class.clone(),
        StaticReceiver::Parent => ctx
            .current_class
            .as_ref()
            .and_then(|current_class| ctx.classes.get(current_class))
            .and_then(|class_info| class_info.parent.clone()),
    }
}

/// Resolves a class name case-insensitively for metadata lookups.
fn resolve_class_name<'a>(ctx: &'a Context, class_name: &str) -> Option<&'a str> {
    let class_key = php_symbol_key(class_name.trim_start_matches('\\'));
    ctx.classes
        .keys()
        .find(|existing| php_symbol_key(existing) == class_key)
        .map(String::as_str)
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
