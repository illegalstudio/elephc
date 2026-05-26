//! Purpose:
//! Inlines `value |> (fn($v) => body)` pipes whose closure target is a captureless,
//! single-parameter, single-return arrow/closure. Substitutes the parameter with
//! the piped value and replaces the entire pipe with the resulting expression.
//!
//! Called from:
//! - `crate::optimize::fold::expr::fold_expr` (Pipe branch), after the pure-builtin
//!   constant fold has been tried.
//!
//! Key details:
//! - Only single-parameter, captureless, non-variadic, non-ref-param closures whose
//!   body is exactly one `return <expr>;` are eligible. Reference and capture
//!   semantics are surface-visible at runtime, so the inliner refuses them.
//! - When the parameter appears more than once in the body and the value is not a
//!   trivial literal, the substitution is skipped to avoid duplicating side effects
//!   or evaluating a non-trivial expression more than once.
//! - The substitution walk is restricted to a safe subset of `ExprKind`s. Anything
//!   we cannot reason about (calls, accesses, ternaries with closures, etc.) makes
//!   the helper bail and falls back to the regular pipe lowering.

use crate::parser::ast::{Expr, ExprKind, Stmt, StmtKind};

/// Attempts to inline a `value |> (fn($v) => body)` pipe into a direct expression.
///
/// Returns `Some(ExprKind)` with the substituted body if the closure is eligible for
/// inlining, or `None` if any eligibility check fails.
///
/// Eligibility: captureless, single non-ref param, single-return body, no variadic,
/// non-static closure. Also skips inlining when the parameter is used multiple times
/// and the value is non-trivial (avoids duplicating side effects or re-evaluating).
pub(super) fn try_inline_closure_pipe(value: &Expr, callable: &Expr) -> Option<ExprKind> {
    let (params, body, captures, variadic, is_static) = match &callable.kind {
        ExprKind::Closure {
            params,
            body,
            captures,
            variadic,
            is_static,
            ..
        } => (params, body, captures, variadic, is_static),
        _ => return None,
    };

    if !captures.is_empty() || variadic.is_some() || params.len() != 1 || *is_static {
        return None;
    }
    let (param_name, _type, _default, by_ref) = &params[0];
    if *by_ref {
        return None;
    }

    let body_expr = match body.as_slice() {
        [Stmt {
            kind: StmtKind::Return(Some(expr)),
            ..
        }] => expr,
        _ => return None,
    };

    if expr_contains_call(body_expr) {
        return None;
    }

    let uses = count_uses(body_expr, param_name);
    if uses == 0 {
        // Parameter is unused → just return the body expression unchanged.
        // Still safe because `value` cannot have observable effects that the
        // closure would have evaluated (we bail in that case below).
        if !value_is_pure(value) {
            return None;
        }
        return Some(body_expr.kind.clone());
    }
    if uses > 1 && !value_is_trivial(value) {
        return None;
    }

    let substituted = substitute(body_expr, param_name, value)?;
    Some(substituted.kind)
}

/// Returns true if the expression is a trivial literal or variable (no observable
/// side effects, cheap to duplicate).
fn value_is_trivial(value: &Expr) -> bool {
    matches!(
        &value.kind,
        ExprKind::IntLiteral(_)
            | ExprKind::FloatLiteral(_)
            | ExprKind::StringLiteral(_)
            | ExprKind::BoolLiteral(_)
            | ExprKind::Null
            | ExprKind::Variable(_)
    )
}

/// Returns true if the expression is a pure literal (no variable reads, no observable
/// side effects when dropped or copied). Super-conservative; excludes variables because
/// reading them is observable in ownership-sensitive contexts.
fn value_is_pure(value: &Expr) -> bool {
    // A super-conservative pure check: only literals. Variables are excluded
    // because reading them is itself observable when ownership matters
    // (e.g., dropping a borrow).
    matches!(
        &value.kind,
        ExprKind::IntLiteral(_)
            | ExprKind::FloatLiteral(_)
            | ExprKind::StringLiteral(_)
            | ExprKind::BoolLiteral(_)
            | ExprKind::Null
    )
}

/// Counts how many times `name` appears as a bare `Variable` node in the expression
/// tree. Used to decide whether substituting a non-trivial value would duplicate
/// side effects or re-evaluate a complex expression.
fn count_uses(expr: &Expr, name: &str) -> usize {
    match &expr.kind {
        ExprKind::Variable(n) if n == name => 1,
        ExprKind::BinaryOp { left, right, .. } => count_uses(left, name) + count_uses(right, name),
        ExprKind::Negate(inner)
        | ExprKind::Not(inner)
        | ExprKind::BitNot(inner)
        | ExprKind::Cast { expr: inner, .. } => count_uses(inner, name),
        ExprKind::NullCoalesce { value, default } => {
            count_uses(value, name) + count_uses(default, name)
        }
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => count_uses(condition, name) + count_uses(then_expr, name) + count_uses(else_expr, name),
        ExprKind::ShortTernary { value, default } => {
            count_uses(value, name) + count_uses(default, name)
        }
        ExprKind::Pipe { value, callable } => count_uses(value, name) + count_uses(callable, name),
        ExprKind::FunctionCall { args, .. } => args.iter().map(|a| count_uses(a, name)).sum(),
        _ => 0,
    }
}

/// Returns true if the expression tree contains any call-like expression (`FunctionCall`,
/// `MethodCall`, `ClosureCall`, `NewObject`, etc.) or a `Pipe` (which may carry calls
/// in its callable operand). Used to bail out of inlining when the body contains
/// observable call side effects that could be duplicated or mis-ordered.
fn expr_contains_call(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::FunctionCall { .. }
        | ExprKind::ClosureCall { .. }
        | ExprKind::ExprCall { .. }
        | ExprKind::MethodCall { .. }
        | ExprKind::NullsafeMethodCall { .. }
        | ExprKind::StaticMethodCall { .. }
        | ExprKind::NewObject { .. }
        | ExprKind::NewScopedObject { .. } => true,
        ExprKind::BinaryOp { left, right, .. } => {
            expr_contains_call(left) || expr_contains_call(right)
        }
        ExprKind::Negate(inner)
        | ExprKind::Not(inner)
        | ExprKind::BitNot(inner)
        | ExprKind::Cast { expr: inner, .. } => expr_contains_call(inner),
        ExprKind::NullCoalesce { value, default }
        | ExprKind::ShortTernary { value, default } => {
            expr_contains_call(value) || expr_contains_call(default)
        }
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            expr_contains_call(condition)
                || expr_contains_call(then_expr)
                || expr_contains_call(else_expr)
        }
        ExprKind::Pipe { .. } => true,
        _ => false,
    }
}

/// Replaces all occurrences of `name` as a `Variable` node in `expr` with `with`,
/// returning the substituted expression. Only operates on a safe subset of `ExprKind`s
/// (literals, variables, binary ops, unary ops, casts, null-coalesce, ternary,
/// short-ternary, pipe, function calls). Returns `None` if any visited node falls
/// outside the safe subset, in which case the caller should fall back to regular
/// pipe lowering.
fn substitute(expr: &Expr, name: &str, with: &Expr) -> Option<Expr> {
    let kind = match &expr.kind {
        ExprKind::Variable(n) if n == name => return Some(with.clone()),
        kind @ (ExprKind::IntLiteral(_)
        | ExprKind::FloatLiteral(_)
        | ExprKind::StringLiteral(_)
        | ExprKind::BoolLiteral(_)
        | ExprKind::Null
        | ExprKind::Variable(_)) => kind.clone(),
        ExprKind::BinaryOp { left, op, right } => ExprKind::BinaryOp {
            left: Box::new(substitute(left, name, with)?),
            op: op.clone(),
            right: Box::new(substitute(right, name, with)?),
        },
        ExprKind::Negate(inner) => ExprKind::Negate(Box::new(substitute(inner, name, with)?)),
        ExprKind::Not(inner) => ExprKind::Not(Box::new(substitute(inner, name, with)?)),
        ExprKind::BitNot(inner) => ExprKind::BitNot(Box::new(substitute(inner, name, with)?)),
        ExprKind::Cast { target, expr: inner } => ExprKind::Cast {
            target: target.clone(),
            expr: Box::new(substitute(inner, name, with)?),
        },
        ExprKind::NullCoalesce { value, default } => ExprKind::NullCoalesce {
            value: Box::new(substitute(value, name, with)?),
            default: Box::new(substitute(default, name, with)?),
        },
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => ExprKind::Ternary {
            condition: Box::new(substitute(condition, name, with)?),
            then_expr: Box::new(substitute(then_expr, name, with)?),
            else_expr: Box::new(substitute(else_expr, name, with)?),
        },
        ExprKind::ShortTernary { value, default } => ExprKind::ShortTernary {
            value: Box::new(substitute(value, name, with)?),
            default: Box::new(substitute(default, name, with)?),
        },
        ExprKind::Pipe { value, callable } => ExprKind::Pipe {
            value: Box::new(substitute(value, name, with)?),
            callable: Box::new(substitute(callable, name, with)?),
        },
        ExprKind::FunctionCall { name: fn_name, args } => {
            let args: Option<Vec<Expr>> = args.iter().map(|a| substitute(a, name, with)).collect();
            ExprKind::FunctionCall {
                name: fn_name.clone(),
                args: args?,
            }
        }
        _ => return None,
    };
    Some(Expr::new(kind, expr.span))
}
