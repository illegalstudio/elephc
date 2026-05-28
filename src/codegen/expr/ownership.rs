//! Purpose:
//! Classifies expression results as owned, borrowed, persistent, or non-refcounted for cleanup decisions.
//! Provides retain and release helpers used around heap-valued temporaries and arguments.
//!
//! Called from:
//! - `crate::codegen::expr` and statement cleanup paths
//!
//! Key details:
//! - Ownership answers must stay conservative to avoid leaks, double frees, and borrowed-value releases.

use crate::codegen::context::{Context, HeapOwnership};
use crate::parser::ast::{BinOp, CastType, Expr, ExprKind};
use crate::types::PhpType;

/// Classifies what kind of heap ownership an expression result carries.
///
/// Returns `Owned` for heap-allocated values that the callee owns (strings, arrays,
/// objects, call results). Returns `Borrowed` for variables and property accesses
/// that alias existing storage. Returns `Persistent` for values that outlive the
/// current scope. Returns `NonHeap` for scalars that need no cleanup.
///
/// This is used to decide whether to emit `retain` or `release` around expression
/// results in codegen cleanup paths.
pub(crate) fn expr_result_heap_ownership(expr: &Expr) -> HeapOwnership {
    match &expr.kind {
        ExprKind::Variable(_)
        | ExprKind::ArrayAccess { .. }
        | ExprKind::PropertyAccess { .. }
        | ExprKind::DynamicPropertyAccess { .. }
        | ExprKind::NullsafePropertyAccess { .. }
        | ExprKind::NullsafeDynamicPropertyAccess { .. }
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
        | ExprKind::Closure { .. }
        | ExprKind::FirstClassCallable(_)
        | ExprKind::FunctionCall { .. }
        | ExprKind::ClosureCall { .. }
        | ExprKind::ExprCall { .. }
        | ExprKind::MethodCall { .. }
        | ExprKind::NullsafeMethodCall { .. }
        | ExprKind::StaticMethodCall { .. }
        | ExprKind::NewObject { .. } => HeapOwnership::Owned,
        ExprKind::BinaryOp { op, .. } => {
            if matches!(op, BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Concat) {
                HeapOwnership::Owned
            } else {
                HeapOwnership::NonHeap
            }
        }
        _ => HeapOwnership::NonHeap,
    }
}

/// Returns `true` if the expression produces a string result that uses a transient
/// concatenation buffer (stack-allocated, not leak-safe across yields or exceptions).
///
/// This applies to binary concat operations and to casts/spreads/error-suppress
/// wrappers around such concat chains. Codegen uses this to decide whether to copy
/// the result before a potential yield or control-flow merge.
pub(crate) fn string_result_uses_transient_concat_buffer(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::BinaryOp {
            op: BinOp::Concat, ..
        } => true,
        ExprKind::Spread(inner)
        | ExprKind::PtrCast { expr: inner, .. }
        | ExprKind::ErrorSuppress(inner) => string_result_uses_transient_concat_buffer(inner),
        ExprKind::Cast {
            target: CastType::String,
            expr: inner,
        } => string_result_uses_transient_concat_buffer(inner),
        _ => false,
    }
}

/// Returns `true` if a call expression produces an owned string temporary that
/// requires release when the call's result is not consumed.
///
/// This is true for user-defined function calls that return strings (non-extern,
/// non-builtin), method calls, closure calls, and expressions that involve types
/// with `__toString`. Builtin functions that return owned strings are also included.
pub(crate) fn string_result_is_owned_call_temp(value: &Expr, ctx: &Context) -> bool {
    match &value.kind {
        ExprKind::FunctionCall { name, .. } => {
            let name = name.as_str();
            builtin_returns_owned_string(name)
                || (ctx.functions.contains_key(name)
                    && !ctx.extern_functions.contains_key(name)
                    && !crate::types::checker::builtins::is_supported_builtin_function(name))
        }
        ExprKind::MethodCall { .. }
        | ExprKind::NullsafeMethodCall { .. }
        | ExprKind::StaticMethodCall { .. }
        | ExprKind::ClosureCall { .. }
        | ExprKind::ExprCall { .. } => true,
        ExprKind::Cast {
            target: CastType::String,
            expr,
        } => string_result_is_owned_call_temp(expr, ctx),
        ExprKind::Variable(name) => ctx
            .variables
            .get(name)
            .is_some_and(|var| {
                type_has_tostring(&var.ty, ctx) || type_has_tostring(&var.static_ty, ctx)
            }),
        ExprKind::This => ctx
            .current_class
            .as_deref()
            .is_some_and(|class_name| class_has_tostring(ctx, class_name)),
        ExprKind::NewObject { class_name, .. } => class_has_tostring(ctx, class_name.as_str()),
        _ => false,
    }
}

/// Returns `true` for builtin functions that return an owned heap-allocated string
/// requiring release.
///
/// Currently only `ptr_read_string` returns owned strings among builtins.
fn builtin_returns_owned_string(name: &str) -> bool {
    matches!(name, "ptr_read_string")
}

/// Returns `true` if a PHP type has a `__toString` method, making its runtime
/// representation an owned string when coerced.
fn type_has_tostring(ty: &PhpType, ctx: &Context) -> bool {
    match ty.codegen_repr() {
        PhpType::Object(class_name) => class_has_tostring(ctx, &class_name),
        _ => false,
    }
}

/// Returns `true` if a class defines a `__toString` method.
fn class_has_tostring(ctx: &Context, class_name: &str) -> bool {
    ctx.classes
        .get(class_name)
        .is_some_and(|class_info| class_info.methods.contains_key("__tostring"))
}
