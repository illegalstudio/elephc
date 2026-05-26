//! Purpose:
//! Implements optimizer control-flow switch logic.
//! Supports normalization, reachability, path analysis, and structural rewrites used by pruning and DCE.
//!
//! Called from:
//! - `crate::optimize::control`
//!
//! Key details:
//! - Control-flow helpers must treat terminal effects, switch fallthrough, and exception paths conservatively.

use super::*;

/// Optimizes a `switch` statement by folding known subject values, pruning unreachable cases,
/// and rewriting level-sensitive switches that cannot be safely normalized.
///
/// - `subject` is pruned before analysis.
/// - Cases and default branch are normalized and pruned.
/// - Returns the execution path for a known subject value, or the original switch if
///   level-sensitive exits prevent safe rewriting, or if the subject is not scalar.
pub(crate) fn prune_switch_stmt(
    subject: Expr,
    cases: Vec<(Vec<Expr>, Vec<Stmt>)>,
    default: Option<Vec<Stmt>>,
    span: crate::span::Span,
) -> Vec<Stmt> {
    let subject = prune_expr(subject);
    let cases = normalize_switch_cases(drop_shadowed_switch_patterns(normalize_switch_cases(
        cases
            .into_iter()
            .map(|(patterns, body)| {
                (patterns.into_iter().map(prune_expr).collect(), prune_block(body))
            })
            .collect(),
    )));
    let default = normalize_optional_block(default.map(prune_block));

    if cases.iter().all(|(_, body)| body.is_empty()) && default.is_none() {
        return expr_to_effect_stmt(subject);
    }

    if switch_has_level_sensitive_loop_exit(&cases, &default) {
        return vec![Stmt {
            kind: StmtKind::Switch {
                subject,
                cases,
                default,
            },
            span,
            attributes: Vec::new(),
        }];
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
            attributes: Vec::new(),
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
                    attributes: Vec::new(),
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

/// Optimizes a `match` expression by folding a known scalar subject value into the arms.
///
/// Returns the result expression for the first matching arm, the default expression if
/// the subject matches no arms, or the original `ExprKind::Match` if any arm classification
/// is unknown or the subject is non-scalar.
pub(crate) fn try_prune_match_expr(
    subject: Expr,
    arms: Vec<(Vec<Expr>, Expr)>,
    default: Option<Box<Expr>>,
) -> ExprKind {
    let arms = drop_shadowed_match_arms(arms);
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

/// Removes `match` arms whose patterns are already covered by earlier arms.
///
/// Duplicates are detected via structural equality of expressions.
/// Arms with empty pattern lists are skipped.
fn drop_shadowed_match_arms(arms: Vec<(Vec<Expr>, Expr)>) -> Vec<(Vec<Expr>, Expr)> {
    let mut normalized = Vec::new();
    let mut seen_patterns: Vec<Expr> = Vec::new();

    for (mut patterns, value) in arms {
        patterns.retain(|pattern| {
            if seen_patterns.iter().any(|seen| seen == pattern) {
                false
            } else {
                seen_patterns.push(pattern.clone());
                true
            }
        });

        if patterns.is_empty() {
            continue;
        }

        normalized.push((patterns, value));
    }

    normalized
}

/// Classification of how a switch/case pattern matches a subject value.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum CaseMatch {
    /// The subject value provably matches this pattern.
    Matches,
    /// The subject value provably does not match this pattern.
    NoMatch,
    /// Whether the subject matches cannot be determined at compile time.
    Unknown,
}

/// Comparison mode for switch/case pattern matching, affecting type coercion behavior.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum CaseComparison {
    /// Strict equality: booleans match only booleans, null matches only null.
    Strict,
    /// Loose PHP-style switch comparison: int/float coerce to same numeric value,
    /// strings compare by value, mixed types yield Unknown.
    LooseSwitch,
}

/// Classifies whether a scalar subject value matches, does not match, or is indeterminate
/// relative to a list of case patterns under the given comparison mode.
///
/// Iterates over patterns and returns early on the first definite match or unknown.
/// Returns `Unknown` if any pattern yields `None` from `pattern_matches_scalar`.
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

/// Determines if a case pattern matches a scalar subject value under the given comparison mode.
///
/// Returns `Some(true)` if the pattern matches, `Some(false)` if it does not,
/// or `None` if the result cannot be determined (e.g., float compared to string).
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

/// Strict equality comparison between two scalar values.
///
/// Returns `Some(true)` for matching pairs, `Some(false)` for mismatched pairs,
/// or `Some(false)` for cross-type comparisons (e.g., int vs string).
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

/// Loose PHP-style switch comparison between two scalar values.
///
/// String compares by value; float compares by numeric value; int is extracted via
/// `scalar_dispatch_int`. Cross-type comparisons between string/float and other types
/// yield `None` (indeterminate).
pub(crate) fn compare_scalar_switch(left: &ScalarValue, right: &ScalarValue) -> Option<bool> {
    match (left, right) {
        (ScalarValue::String(left), ScalarValue::String(right)) => Some(left == right),
        (ScalarValue::Float(left), ScalarValue::Float(right)) => Some(left == right),
        (ScalarValue::String(_), _) | (_, ScalarValue::String(_)) => None,
        (ScalarValue::Float(_), _) | (_, ScalarValue::Float(_)) => None,
        _ => Some(scalar_dispatch_int(left)? == scalar_dispatch_int(right)?),
    }
}

/// Converts a scalar value to an integer for switch dispatch purposes.
///
/// Returns `Some(i64)` for Null (as 0), Bool (0/1), and Int values.
/// Returns `None` for Float and String, which cannot be safely coerced in this context.
pub(crate) fn scalar_dispatch_int(value: &ScalarValue) -> Option<i64> {
    match value {
        ScalarValue::Null => Some(0),
        ScalarValue::Bool(value) => Some(i64::from(*value)),
        ScalarValue::Int(value) => Some(*value),
        ScalarValue::Float(_) | ScalarValue::String(_) => None,
    }
}
