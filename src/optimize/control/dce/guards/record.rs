//! Purpose:
//! Evaluates and records DCE guard record facts.
//! Tracks branch conditions that justify pruning impossible or already-covered control-flow regions.
//!
//! Called from:
//! - `crate::optimize::control::dce::guards`
//!
//! Key details:
//! - Guard facts are path-sensitive and must be forgotten at merges where later writes can change truthiness.

use super::eval::{
    guard_literal_truthy,
    guard_variable_name,
    known_condition_value,
    scalar_guard_value,
    strict_scalar_guard,
};
use super::super::*;
use super::super::state::{ConditionGuard, ExactGuard, GuardLiteral, GuardState};

pub(in crate::optimize::control::dce) fn clear_guards_for_name(guards: &mut GuardState, name: &str) {
    guards.truthy_vars.retain(|known| known != name);
    guards.falsy_vars.retain(|known| known != name);
    guards.bool_true_vars.retain(|known| known != name);
    guards.bool_false_vars.retain(|known| known != name);
    guards.exact_guards.retain(|known| known.name != name);
    guards.excluded_guards.retain(|known| known.name != name);
    guards
        .condition_guards
        .retain(|known| !known.names.iter().any(|known_name| known_name == name));
}
fn push_guard_name(names: &mut Vec<String>, name: &str) {
    if !names.iter().any(|known| known == name) {
        names.push(name.to_string());
    }
}

fn record_truthy_guard(guards: &mut GuardState, name: &str, known_truthy: bool) {
    guards.truthy_vars.retain(|known| known != name);
    guards.falsy_vars.retain(|known| known != name);
    if known_truthy {
        push_guard_name(&mut guards.truthy_vars, name);
    } else {
        push_guard_name(&mut guards.falsy_vars, name);
    }
}

fn record_exact_literal_guard(guards: &mut GuardState, name: &str, value: GuardLiteral) {
    clear_guards_for_name(guards, name);
    if let GuardLiteral::Bool(value) = &value {
        if *value {
            push_guard_name(&mut guards.bool_true_vars, name);
        } else {
            push_guard_name(&mut guards.bool_false_vars, name);
        }
    }
    guards.exact_guards.push(ExactGuard {
        name: name.to_string(),
        value: value.clone(),
    });
    record_truthy_guard(guards, name, guard_literal_truthy(&value));
}

fn exact_literal_from_guard_branch(condition: &Expr, branch_taken: bool) -> Option<(&str, GuardLiteral)> {
    let (name, compared_value, expects_equal) = strict_scalar_guard(condition)?;
    match (expects_equal, branch_taken) {
        (true, true) => Some((name, compared_value)),
        (false, false) => Some((name, compared_value)),
        _ => None,
    }
}

fn excluded_literal_from_guard_branch(
    condition: &Expr,
    branch_taken: bool,
) -> Option<(&str, GuardLiteral)> {
    let (name, compared_value, expects_equal) = strict_scalar_guard(condition)?;
    match (expects_equal, branch_taken) {
        (true, false) => Some((name, compared_value)),
        (false, true) => Some((name, compared_value)),
        _ => None,
    }
}

fn record_excluded_literal_guard(guards: &mut GuardState, name: &str, value: GuardLiteral) {
    if !guards
        .excluded_guards
        .iter()
        .any(|known| known.name == name && known.value == value)
    {
        guards.excluded_guards.push(ExactGuard {
            name: name.to_string(),
            value,
        });
    }
}

fn collect_trackable_condition_names(expr: &Expr, names: &mut Vec<String>) -> bool {
    match &expr.kind {
        ExprKind::Variable(name) => {
            push_guard_name(names, name);
            true
        }
        ExprKind::BoolLiteral(_)
        | ExprKind::Null
        | ExprKind::IntLiteral(_)
        | ExprKind::FloatLiteral(_)
        | ExprKind::StringLiteral(_) => true,
        ExprKind::Not(inner) | ExprKind::Negate(inner) | ExprKind::BitNot(inner) => {
            collect_trackable_condition_names(inner, names)
        }
        ExprKind::BinaryOp { left, right, .. } => {
            collect_trackable_condition_names(left, names)
                && collect_trackable_condition_names(right, names)
        }
        _ => false,
    }
}

fn inverse_comparison_op(op: &BinOp) -> Option<BinOp> {
    match op {
        BinOp::Eq => Some(BinOp::NotEq),
        BinOp::NotEq => Some(BinOp::Eq),
        BinOp::StrictEq => Some(BinOp::StrictNotEq),
        BinOp::StrictNotEq => Some(BinOp::StrictEq),
        BinOp::Lt => Some(BinOp::GtEq),
        BinOp::Gt => Some(BinOp::LtEq),
        BinOp::LtEq => Some(BinOp::Gt),
        BinOp::GtEq => Some(BinOp::Lt),
        _ => None,
    }
}

fn comparison_inverse_is_total(op: &BinOp) -> bool {
    matches!(op, BinOp::Eq | BinOp::NotEq | BinOp::StrictEq | BinOp::StrictNotEq)
}

fn condition_guard_forms(condition: &Expr, value: bool) -> Vec<(Expr, bool)> {
    let mut forms = Vec::new();

    match &condition.kind {
        ExprKind::Not(inner) => {
            if let ExprKind::BinaryOp { left, op, right } = &inner.kind {
                let de_morgan_op = match op {
                    BinOp::And => Some(BinOp::Or),
                    BinOp::Or => Some(BinOp::And),
                    _ => None,
                };

                if let Some(de_morgan_op) = de_morgan_op {
                    forms.push((
                        Expr::binop(
                            invert_condition((**left).clone()),
                            de_morgan_op,
                            invert_condition((**right).clone()),
                        ),
                        value,
                    ));
                }
            }
        }
        ExprKind::BinaryOp { left, op, right } => {
            if let Some(inverse_op) = inverse_comparison_op(op) {
                if value || comparison_inverse_is_total(op) {
                    forms.push((
                        Expr::binop((**left).clone(), inverse_op, (**right).clone()),
                        !value,
                    ));
                }
            }

            let de_morgan_op = match op {
                BinOp::And => Some(BinOp::Or),
                BinOp::Or => Some(BinOp::And),
                _ => None,
            };

            if let (
                Some(de_morgan_op),
                ExprKind::Not(left_inner),
                ExprKind::Not(right_inner),
            ) = (de_morgan_op, &left.kind, &right.kind)
            {
                forms.push((
                    invert_condition(Expr::binop(
                        (**left_inner).clone(),
                        de_morgan_op,
                        (**right_inner).clone(),
                    )),
                    value,
                ));
            }
        }
        _ => {}
    }

    forms
}

fn upsert_condition_guard(
    guards: &mut GuardState,
    condition: Expr,
    value: bool,
    names: &[String],
) {
    if let Some(existing) = guards
        .condition_guards
        .iter_mut()
        .find(|known| known.condition == condition)
    {
        existing.value = value;
        existing.names = names.to_vec();
        return;
    }

    guards.condition_guards.push(ConditionGuard {
        condition,
        value,
        names: names.to_vec(),
    });
}

fn record_condition_guard(guards: &mut GuardState, condition: &Expr, value: bool) {
    let effect = expr_effect(condition);
    if effect.has_side_effects || effect.may_throw {
        return;
    }

    let mut names = Vec::new();
    if !collect_trackable_condition_names(condition, &mut names) {
        return;
    }

    upsert_condition_guard(guards, condition.clone(), value, &names);
    for (equivalent, equivalent_value) in condition_guard_forms(condition, value) {
        let equivalent_effect = expr_effect(&equivalent);
        if equivalent_effect.has_side_effects || equivalent_effect.may_throw {
            continue;
        }
        upsert_condition_guard(guards, equivalent, equivalent_value, &names);
    }
}

pub(in crate::optimize::control::dce) fn extend_guards_for_switch_case(subject: &Expr, patterns: &[Expr], guards: &GuardState) -> GuardState {
    let [pattern] = patterns else {
        return guards.clone();
    };

    match &subject.kind {
        ExprKind::BoolLiteral(subject_bool) => extend_guards(guards, pattern, *subject_bool),
        ExprKind::Variable(name) => {
            let mut next = guards.clone();
            if let ExprKind::BoolLiteral(pattern_bool) = pattern.kind {
                record_truthy_guard(&mut next, name, pattern_bool);
            }
            next
        }
        _ => guards.clone(),
    }
}

pub(in crate::optimize::control::dce) fn extend_guards_for_switch_case_no_match(
    subject_value: &ScalarValue,
    patterns: &[Expr],
    guards: &GuardState,
) -> GuardState {
    let ScalarValue::Bool(subject_bool) = subject_value else {
        return guards.clone();
    };

    patterns.iter().fold(guards.clone(), |guards, pattern| {
        extend_guards(&guards, pattern, !subject_bool)
    })
}

pub(in crate::optimize::control::dce) fn extend_guards_for_switch_case_no_match_subject(
    subject: &Expr,
    patterns: &[Expr],
    guards: &GuardState,
) -> GuardState {
    if let Some(subject_value) = known_scalar_subject_value(subject, guards) {
        if matches!(subject_value, ScalarValue::Bool(_)) {
            return extend_guards_for_switch_case_no_match(&subject_value, patterns, guards);
        }
    }

    let ExprKind::Variable(name) = &subject.kind else {
        return guards.clone();
    };

    patterns.iter().fold(guards.clone(), |mut guards, pattern| {
        match &pattern.kind {
            ExprKind::BoolLiteral(pattern_bool) => {
                record_truthy_guard(&mut guards, name, !pattern_bool);
            }
            _ => {
                if let Some(pattern_value) = scalar_guard_value(pattern) {
                    record_excluded_literal_guard(&mut guards, name, pattern_value);
                }
            }
        }
        guards
    })
}

pub(in crate::optimize::control::dce) fn extend_guards(guards: &GuardState, condition: &Expr, branch_taken: bool) -> GuardState {
    let mut next = if let ExprKind::Not(inner) = &condition.kind {
        extend_guards(guards, inner, !branch_taken)
    } else if let ExprKind::BinaryOp { left, op, right } = &condition.kind {
        match (op, branch_taken) {
            (BinOp::And, true) => {
                let left_true = extend_guards(guards, left, true);
                extend_guards(&left_true, right, true)
            }
            (BinOp::And, false) => {
                if matches!(known_condition_value(left, guards), Some(true)) {
                    let left_true = extend_guards(guards, left, true);
                    extend_guards(&left_true, right, false)
                } else {
                    guards.clone()
                }
            }
            (BinOp::Or, false) => {
                let left_false = extend_guards(guards, left, false);
                extend_guards(&left_false, right, false)
            }
            (BinOp::Or, true) => {
                if matches!(known_condition_value(left, guards), Some(false)) {
                    let left_false = extend_guards(guards, left, false);
                    extend_guards(&left_false, right, true)
                } else {
                    guards.clone()
                }
            }
            _ => guards.clone(),
        }
    } else {
        guards.clone()
    };

    if let Some((name, exact_value)) = exact_literal_from_guard_branch(condition, branch_taken) {
        record_exact_literal_guard(&mut next, name, exact_value);
    }

    if let Some((name, excluded_value)) = excluded_literal_from_guard_branch(condition, branch_taken) {
        record_excluded_literal_guard(&mut next, name, excluded_value);
    }

    record_condition_guard(&mut next, condition, branch_taken);

    if let Some((name, truthy_if_true)) = guard_variable_name(condition) {
        let known_truthy = if branch_taken { truthy_if_true } else { !truthy_if_true };
        record_truthy_guard(&mut next, name, known_truthy);
    };

    next
}
