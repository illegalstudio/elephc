//! Purpose:
//! Evaluates and records DCE guard eval facts.
//! Tracks branch conditions that justify pruning impossible or already-covered control-flow regions.
//!
//! Called from:
//! - `crate::optimize::control::dce::guards`
//!
//! Key details:
//! - Guard facts are path-sensitive and must be forgotten at merges where later writes can change truthiness.

use super::super::*;
use super::super::state::{GuardLiteral, GuardState};

/// Extracts a variable name and its expected truthiness from a simple guard condition.
///
/// Returns `Some((name, truthy_if_true))` where `truthy_if_true` indicates whether
/// the variable is expected to be truthy (true) or falsy (false) when the condition
/// evaluates to true. Handles bare variables (`$x`) and negated variables (`!$x`).
/// Returns `None` for any other expression shape.
pub(in crate::optimize::control::dce) fn guard_variable_name(condition: &Expr) -> Option<(&str, bool)> {
    match &condition.kind {
        ExprKind::Variable(name) => Some((name.as_str(), true)),
        ExprKind::Not(inner) => match &inner.kind {
            ExprKind::Variable(name) => Some((name.as_str(), false)),
            _ => None,
        },
        _ => None,
    }
}
/// Converts a scalar literal expression into a `GuardLiteral`.
///
/// Handles `bool`, `null`, `int`, `float`, and `string` literals.
/// Returns `None` for variables, operators, or any non-literal expression.
pub(in crate::optimize::control::dce) fn scalar_guard_value(expr: &Expr) -> Option<GuardLiteral> {
    match &expr.kind {
        ExprKind::BoolLiteral(value) => Some(GuardLiteral::Bool(*value)),
        ExprKind::Null => Some(GuardLiteral::Null),
        ExprKind::IntLiteral(value) => Some(GuardLiteral::Int(*value)),
        ExprKind::FloatLiteral(value) => Some(GuardLiteral::Float(value.to_bits())),
        ExprKind::StringLiteral(value) => Some(GuardLiteral::String(value.clone())),
        _ => None,
    }
}

/// Matches a strict equality/inequality guard condition against a scalar value.
///
/// Unwraps a `BinaryOp` expression and returns `Some((name, value, expects_equal))` when:
/// - One side is a variable and the other is a scalar literal
/// - The operator is `===` (expects_equal=true) or `!==` (expects_equal=false)
///
/// Used by `exact_literal_from_guard_branch` and `excluded_literal_from_guard_branch`
/// to record precise variable value constraints from guard branches.
pub(in crate::optimize::control::dce) fn strict_scalar_guard(condition: &Expr) -> Option<(&str, GuardLiteral, bool)> {
    let ExprKind::BinaryOp { left, op, right } = &condition.kind else {
        return None;
    };

    let (name, value) = match (&left.kind, &right.kind) {
        (ExprKind::Variable(name), _) => (name.as_str(), scalar_guard_value(right)?),
        (_, ExprKind::Variable(name)) => (name.as_str(), scalar_guard_value(left)?),
        _ => return None,
    };

    match op {
        BinOp::StrictEq => Some((name, value, true)),
        BinOp::StrictNotEq => Some((name, value, false)),
        _ => None,
    }
}

/// Evaluates the truthiness of a `GuardLiteral` value.
///
/// Returns `true` for truthy values (non-zero int/float, non-empty non-"0" string, true),
/// `false` for falsy values (null, zero, empty string, "0", false).
pub(in crate::optimize::control::dce) fn guard_literal_truthy(value: &GuardLiteral) -> bool {
    match value {
        GuardLiteral::Bool(value) => *value,
        GuardLiteral::Null => false,
        GuardLiteral::Int(value) => *value != 0,
        GuardLiteral::Float(bits) => f64::from_bits(*bits) != 0.0,
        GuardLiteral::String(value) => !value.is_empty() && value != "0",
    }
}

/// Looks up a variable's known exact value from `exact_guards`.
///
/// Scans `guards.exact_guards` for an entry with matching `name` and returns
/// a reference to its `GuardLiteral` value, or `None` if no exact value is known.
pub(in crate::optimize::control::dce) fn known_exact_guard<'a>(guards: &'a GuardState, name: &str) -> Option<&'a GuardLiteral> {
    guards
        .exact_guards
        .iter()
        .find(|known| known.name == name)
        .map(|known| &known.value)
}

/// Checks whether a variable has an excluded guard for a specific value.
///
/// Returns `true` if `guards.excluded_guards` contains an entry with the given
/// `name` and `value`, indicating the variable provably cannot equal that value.
pub(in crate::optimize::control::dce) fn has_excluded_guard(guards: &GuardState, name: &str, value: &GuardLiteral) -> bool {
    guards
        .excluded_guards
        .iter()
        .any(|known| known.name == name && known.value == *value)
}

/// Entry point for determining whether a guard condition has a known boolean value.
///
/// Wraps `known_condition_value_inner` with a fresh `visiting` set to detect
/// cyclic references (e.g., `while ($x && !$x)`) and short-circuit recursion.
pub(in crate::optimize::control::dce) fn known_condition_value(condition: &Expr, guards: &GuardState) -> Option<bool> {
    let mut visiting = Vec::new();
    known_condition_value_inner(condition, guards, &mut visiting)
}

/// Recursively evaluates whether a condition is known to be true or false under the guard state.
///
/// Uses `visiting` to track expressions currently on the recursion stack and avoid cycles.
/// First checks direct `condition_guards` and `Not`/`And`/`Or` short-circuit logic,
/// then falls back to `infer_condition_value_from_composite_guards` to derive values
/// from composite guards that contain the condition as a subexpression.
pub(in crate::optimize::control::dce) fn known_condition_value_inner(
    condition: &Expr,
    guards: &GuardState,
    visiting: &mut Vec<Expr>,
) -> Option<bool> {
    if visiting.iter().any(|known| known == condition) {
        return None;
    }

    visiting.push(condition.clone());
    let value = if let Some(value) = known_condition_value_base(condition, guards, visiting) {
        Some(value)
    } else {
        infer_condition_value_from_composite_guards(condition, guards, visiting)
    };
    visiting.pop();

    value
}

/// Core evaluator for direct condition lookups in `GuardState`.
///
/// Checks, in order:
/// 1. `condition_guards` — explicit recorded conditions
/// 2. `Not` — recursively evaluates the inner expression and inverts
/// 3. `And`/`Or` — short-circuit evaluation using recursive `known_condition_value_inner`
/// 4. Variable truthiness via `guard_variable_name` against `truthy_vars`/`falsy_vars`
/// 5. Strict equality guards via `strict_scalar_guard` against `exact_guards`/`excluded_guards`
///
/// Returns `Some(true)`, `Some(false)`, or `None` if the value cannot be determined.
pub(in crate::optimize::control::dce) fn known_condition_value_base(
    condition: &Expr,
    guards: &GuardState,
    visiting: &mut Vec<Expr>,
) -> Option<bool> {
    if let Some(value) = guards
        .condition_guards
        .iter()
        .find(|known| known.condition == *condition)
        .map(|known| known.value)
    {
        return Some(value);
    }

    if let ExprKind::Not(inner) = &condition.kind {
        return known_condition_value_inner(inner, guards, visiting).map(|value| !value);
    }

    if let ExprKind::BinaryOp { left, op, right } = &condition.kind {
        match op {
            BinOp::And => match (
                known_condition_value_inner(left, guards, visiting),
                known_condition_value_inner(right, guards, visiting),
            ) {
                (Some(false), _) | (_, Some(false)) => return Some(false),
                (Some(true), Some(true)) => return Some(true),
                _ => {}
            },
            BinOp::Or => match (
                known_condition_value_inner(left, guards, visiting),
                known_condition_value_inner(right, guards, visiting),
            ) {
                (Some(true), _) | (_, Some(true)) => return Some(true),
                (Some(false), Some(false)) => return Some(false),
                _ => {}
            },
            _ => {}
        }
    }

    if let Some((name, truthy_if_true)) = guard_variable_name(condition) {
        if let Some(value) = known_exact_guard(guards, name) {
            return Some(guard_literal_truthy(value) == truthy_if_true);
        }
        if guards.bool_true_vars.iter().any(|known| known == name)
            || guards.truthy_vars.iter().any(|known| known == name)
        {
            return Some(truthy_if_true);
        }
        if guards.bool_false_vars.iter().any(|known| known == name)
            || guards.falsy_vars.iter().any(|known| known == name)
        {
            return Some(!truthy_if_true);
        }
    }

    if let Some((name, compared_value, expects_equal)) = strict_scalar_guard(condition) {
        if let Some(known) = known_exact_guard(guards, name) {
            return Some((known == &compared_value) == expects_equal);
        }
        if has_excluded_guard(guards, name, &compared_value) {
            return Some(!expects_equal);
        }
    }

    None
}

/// Returns `true` if `target` is a subexpression of `expr`, recursively.
///
/// Performs structural comparison across `Not`, `Negate`, `BitNot`, and `BinaryOp`
/// nodes. Used to determine whether a condition appears within a composite guard.
fn expr_contains_subexpr(expr: &Expr, target: &Expr) -> bool {
    if expr == target {
        return true;
    }

    match &expr.kind {
        ExprKind::Not(inner) | ExprKind::Negate(inner) | ExprKind::BitNot(inner) => {
            expr_contains_subexpr(inner, target)
        }
        ExprKind::BinaryOp { left, right, .. } => {
            expr_contains_subexpr(left, target) || expr_contains_subexpr(right, target)
        }
        _ => false,
    }
}

/// Infers the value of one child operand of a composite guard given the composite's value.
///
/// Used when solving for a sub-condition within an `And` or `Or` expression.
/// For `And`: if composite is true, both children are true; if composite is false
/// and the sibling is true, this child is false.
/// For `Or`: if composite is false, both children are false; if composite is true
/// and the sibling is false, this child is true.
/// Returns `None` when inference is not possible with the given data.
fn infer_child_value_from_composite_guard(
    op: BinOp,
    composite_value: bool,
    sibling_value: Option<bool>,
) -> Option<bool> {
    match (op, composite_value, sibling_value) {
        (BinOp::And, true, _) => Some(true),
        (BinOp::Or, false, _) => Some(false),
        (BinOp::And, false, Some(true)) => Some(false),
        (BinOp::Or, true, Some(false)) => Some(true),
        _ => None,
    }
}

/// Recursively traverses a composite guard tree to extract a sub-condition's value.
///
/// Given a `condition` and a composite `composite` with known `composite_value`,
/// identifies which child of the composite contains the condition (via `expr_contains_subexpr`)
/// and infers the child's value using `infer_child_value_from_composite_guard`.
/// Then recurses into the child to propagate the inferred value, handling `Not` wrappers
/// and `BinaryOp` nodes. Returns `Some(value)` or `None` if traversal fails.
fn infer_condition_value_from_composite_tree(
    condition: &Expr,
    composite: &Expr,
    composite_value: bool,
    guards: &GuardState,
    visiting: &mut Vec<Expr>,
) -> Option<bool> {
    if composite == condition {
        return Some(composite_value);
    }

    if let ExprKind::Not(inner) = &composite.kind {
        return infer_condition_value_from_composite_tree(
            condition,
            inner,
            !composite_value,
            guards,
            visiting,
        );
    }

    let ExprKind::BinaryOp { left, op, right } = &composite.kind else {
        return None;
    };

    let left_contains = expr_contains_subexpr(left, condition);
    let right_contains = expr_contains_subexpr(right, condition);
    let (candidate, sibling) = match (left_contains, right_contains) {
        (true, false) => (&**left, &**right),
        (false, true) => (&**right, &**left),
        _ => return None,
    };

    let candidate_value =
        infer_child_value_from_composite_guard(
            op.clone(),
            composite_value,
            known_condition_value_inner(sibling, guards, visiting),
        )?;

    infer_condition_value_from_composite_tree(
        condition,
        candidate,
        candidate_value,
        guards,
        visiting,
    )
}

/// Searches recorded `condition_guards` for a composite guard containing the target condition.
///
/// Iterates over all `condition_guards` and, for each whose condition contains the target
/// (checked via `expr_contains_subexpr`), calls `infer_condition_value_from_composite_tree`
/// to solve for the target's value. Returns the first found inferred value or `None`.
fn infer_condition_value_from_composite_guards(
    condition: &Expr,
    guards: &GuardState,
    visiting: &mut Vec<Expr>,
) -> Option<bool> {
    for known in &guards.condition_guards {
        if !expr_contains_subexpr(&known.condition, condition) {
            continue;
        }

        if let Some(value) = infer_condition_value_from_composite_tree(
            condition,
            &known.condition,
            known.value,
            guards,
            visiting,
        ) {
            return Some(value);
        }
    }

    None
}
