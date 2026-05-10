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

pub(in crate::optimize::control::dce) fn guard_literal_truthy(value: &GuardLiteral) -> bool {
    match value {
        GuardLiteral::Bool(value) => *value,
        GuardLiteral::Null => false,
        GuardLiteral::Int(value) => *value != 0,
        GuardLiteral::Float(bits) => f64::from_bits(*bits) != 0.0,
        GuardLiteral::String(value) => !value.is_empty() && value != "0",
    }
}

pub(in crate::optimize::control::dce) fn known_exact_guard<'a>(guards: &'a GuardState, name: &str) -> Option<&'a GuardLiteral> {
    guards
        .exact_guards
        .iter()
        .find(|known| known.name == name)
        .map(|known| &known.value)
}

pub(in crate::optimize::control::dce) fn has_excluded_guard(guards: &GuardState, name: &str, value: &GuardLiteral) -> bool {
    guards
        .excluded_guards
        .iter()
        .any(|known| known.name == name && known.value == *value)
}

pub(in crate::optimize::control::dce) fn known_condition_value(condition: &Expr, guards: &GuardState) -> Option<bool> {
    let mut visiting = Vec::new();
    known_condition_value_inner(condition, guards, &mut visiting)
}

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
