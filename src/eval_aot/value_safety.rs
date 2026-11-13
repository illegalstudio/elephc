//! Purpose:
//! Checks string, integer, and float value shapes for EIR AOT.
//!
//! Called from:
//! - The eval AOT facade and sibling analysis modules.
//!
//! Key details:
//! - Scalar recursion remains bounded by the accepted expression subset.

use super::*;

/// Returns true when a value can reach `strlen()` as `Str` or boxed `Mixed`.
pub(super) fn eir_strlen_arg_is_safe<S>(
    expr: &Expr,
    support: &S,
    facts: &EirLocalFacts,
    scope_reads: Option<&BTreeSet<String>>,
) -> bool
where
    S: EirStaticCallSupport,
{
    match &expr.kind {
        ExprKind::StringLiteral(_) => true,
        ExprKind::Variable(name) => {
            facts.is_assigned(name) || scope_reads.is_some_and(|reads| reads.contains(name))
        }
        ExprKind::Cast { target, expr } if matches!(target, CastType::String) => {
            expr_is_eir_function_safe(expr, support, facts, scope_reads)
        }
        _ => false,
    }
}

/// Returns true when an expression is known to produce an integer in the EIR AOT subset.
pub(super) fn expr_is_eir_int_value_safe<S>(
    expr: &Expr,
    support: &S,
    facts: &EirLocalFacts,
    scope_reads: Option<&BTreeSet<String>>,
) -> bool
where
    S: EirStaticCallSupport,
{
    match &expr.kind {
        ExprKind::IntLiteral(_) => true,
        ExprKind::Variable(name) => facts.is_int_local(name),
        ExprKind::Negate(inner) | ExprKind::BitNot(inner) | ExprKind::ErrorSuppress(inner) => {
            expr_is_eir_int_value_safe(inner, support, facts, scope_reads)
        }
        ExprKind::Print(inner) => expr_is_eir_function_safe(inner, support, facts, scope_reads),
        ExprKind::PreIncrement(name)
        | ExprKind::PostIncrement(name)
        | ExprKind::PreDecrement(name)
        | ExprKind::PostDecrement(name) => facts.is_int_local(name),
        ExprKind::Cast { target, expr } if matches!(target, CastType::Int) => {
            expr_is_eir_function_safe(expr, support, facts, scope_reads)
        }
        ExprKind::BinaryOp { left, op, right } => {
            let int_operands = expr_is_eir_int_value_safe(left, support, facts, scope_reads)
                && expr_is_eir_int_value_safe(right, support, facts, scope_reads);
            match op {
                BinOp::Add
                | BinOp::Sub
                | BinOp::Mul
                | BinOp::Mod
                | BinOp::BitAnd
                | BinOp::BitOr
                | BinOp::BitXor
                | BinOp::ShiftLeft
                | BinOp::ShiftRight => int_operands,
                BinOp::Spaceship => {
                    expr_is_eir_function_safe(left, support, facts, scope_reads)
                        && expr_is_eir_function_safe(right, support, facts, scope_reads)
                }
                _ => false,
            }
        }
        ExprKind::FunctionCall { name, args } => {
            fold_static_builtin_int_call(
                name.as_str().trim_start_matches('\\'),
                args,
                support.php_only(),
            )
            .is_some()
        }
        _ => false,
    }
}

/// Returns true when an expression is known to produce a float in the EIR AOT subset.
pub(super) fn expr_is_eir_float_value_safe<S>(
    expr: &Expr,
    support: &S,
    facts: &EirLocalFacts,
    scope_reads: Option<&BTreeSet<String>>,
) -> bool
where
    S: EirStaticCallSupport,
{
    match &expr.kind {
        ExprKind::FloatLiteral(_) => true,
        ExprKind::Variable(name) => facts.is_float_local(name),
        ExprKind::Negate(inner) | ExprKind::ErrorSuppress(inner) => {
            expr_is_eir_float_value_safe(inner, support, facts, scope_reads)
        }
        ExprKind::Cast { target, expr } if matches!(target, CastType::Float) => {
            expr_is_eir_function_safe(expr, support, facts, scope_reads)
        }
        _ => false,
    }
}
