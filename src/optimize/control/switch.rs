use super::*;

pub(crate) fn prune_switch_stmt(
    subject: Expr,
    cases: Vec<(Vec<Expr>, Vec<Stmt>)>,
    default: Option<Vec<Stmt>>,
    span: crate::span::Span,
) -> Vec<Stmt> {
    let subject = prune_expr(subject);
    let cases = normalize_switch_cases(
        cases
            .into_iter()
            .map(|(patterns, body)| {
                (patterns.into_iter().map(prune_expr).collect(), prune_block(body))
            })
            .collect(),
    );
    let default = normalize_optional_block(default.map(prune_block));

    if cases.iter().all(|(_, body)| body.is_empty()) && default.is_none() {
        return expr_to_effect_stmt(subject);
    }

    if cases.is_empty() {
        let mut stmts = expr_to_effect_stmt(subject);
        if let Some(default_body) = default {
            stmts.extend(default_body);
        }
        return stmts;
    }

    let Some(subject_value) = scalar_value(&subject) else {
        if cases.len() == 1 {
            let (patterns, _) = &cases[0];
            if let Some(condition) = build_switch_match_condition(&subject, patterns) {
                let then_body = materialize_switch_execution(&cases, &default, Some(0));
                let else_body =
                    normalize_optional_block(Some(materialize_switch_execution(&cases, &default, None)));
                return prune_if_chain(condition, then_body, Vec::new(), else_body);
            }
        }

        return vec![Stmt {
            kind: StmtKind::Switch {
                subject,
                cases,
                default,
            },
            span,
        }];
    };

    for (index, (patterns, _)) in cases.iter().enumerate() {
        match classify_case_patterns(&subject_value, patterns, CaseComparison::LooseSwitch) {
            CaseMatch::Matches => {
                return materialize_switch_execution(&cases, &default, Some(index));
            }
            CaseMatch::Unknown => {
                return vec![Stmt {
                    kind: StmtKind::Switch {
                        subject,
                        cases: cases[index..].to_vec(),
                        default,
                    },
                    span,
                }];
            }
            CaseMatch::NoMatch => {}
        }
    }

    if default.is_some() {
        materialize_switch_execution(&cases, &default, None)
    } else {
        Vec::new()
    }
}

pub(crate) fn try_prune_match_expr(
    subject: Expr,
    arms: Vec<(Vec<Expr>, Expr)>,
    default: Option<Box<Expr>>,
) -> ExprKind {
    let Some(subject_value) = scalar_value(&subject) else {
        return ExprKind::Match {
            subject: Box::new(subject),
            arms,
            default,
        };
    };

    for (index, (patterns, result)) in arms.iter().enumerate() {
        match classify_case_patterns(&subject_value, patterns, CaseComparison::Strict) {
            CaseMatch::Matches => return result.kind.clone(),
            CaseMatch::NoMatch => {}
            CaseMatch::Unknown => {
                return ExprKind::Match {
                    subject: Box::new(subject),
                    arms: arms[index..].to_vec(),
                    default,
                };
            }
        }
    }

    if let Some(default) = default {
        default.kind
    } else {
        ExprKind::Match {
            subject: Box::new(subject),
            arms: Vec::new(),
            default: None,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum CaseMatch {
    Matches,
    NoMatch,
    Unknown,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum CaseComparison {
    Strict,
    LooseSwitch,
}

pub(crate) fn classify_case_patterns(
    subject: &ScalarValue,
    patterns: &[Expr],
    comparison: CaseComparison,
) -> CaseMatch {
    let mut has_unknown = false;
    for pattern in patterns {
        match pattern_matches_scalar(subject, pattern, comparison) {
            Some(true) => return CaseMatch::Matches,
            Some(false) => {}
            None => has_unknown = true,
        }
    }
    if has_unknown {
        CaseMatch::Unknown
    } else {
        CaseMatch::NoMatch
    }
}

pub(crate) fn pattern_matches_scalar(
    subject: &ScalarValue,
    pattern: &Expr,
    comparison: CaseComparison,
) -> Option<bool> {
    let pattern = scalar_value(pattern)?;
    match comparison {
        CaseComparison::Strict => compare_scalar_strict(subject, &pattern),
        CaseComparison::LooseSwitch => compare_scalar_switch(subject, &pattern),
    }
}

pub(crate) fn compare_scalar_strict(left: &ScalarValue, right: &ScalarValue) -> Option<bool> {
    match (left, right) {
        (ScalarValue::Null, ScalarValue::Null) => Some(true),
        (ScalarValue::Bool(left), ScalarValue::Bool(right)) => Some(left == right),
        (ScalarValue::Int(left), ScalarValue::Int(right)) => Some(left == right),
        (ScalarValue::String(left), ScalarValue::String(right)) => Some(left == right),
        (ScalarValue::Float(left), ScalarValue::Float(right)) => Some(left == right),
        _ => Some(false),
    }
}

pub(crate) fn compare_scalar_switch(left: &ScalarValue, right: &ScalarValue) -> Option<bool> {
    match (left, right) {
        (ScalarValue::String(left), ScalarValue::String(right)) => Some(left == right),
        (ScalarValue::Float(left), ScalarValue::Float(right)) => Some(left == right),
        (ScalarValue::String(_), _) | (_, ScalarValue::String(_)) => None,
        (ScalarValue::Float(_), _) | (_, ScalarValue::Float(_)) => None,
        _ => Some(scalar_dispatch_int(left)? == scalar_dispatch_int(right)?),
    }
}

pub(crate) fn scalar_dispatch_int(value: &ScalarValue) -> Option<i64> {
    match value {
        ScalarValue::Null => Some(0),
        ScalarValue::Bool(value) => Some(i64::from(*value)),
        ScalarValue::Int(value) => Some(*value),
        ScalarValue::Float(_) | ScalarValue::String(_) => None,
    }
}
