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
///   level-sensitive exits prevent safe rewriting, if the subject is not scalar, or if the
///   single-case rewrite would clone a node carrying a checker local-binding decision (see
///   `single_case_rewrite_would_clone_a_decision`).
pub(crate) fn prune_switch_stmt(
    subject: Expr,
    cases: Vec<(Vec<Expr>, Vec<Stmt>)>,
    default: Option<Vec<Stmt>>,
    span: crate::span::Span,
    source_mode: crate::source::SourceMode,
    strict_types: bool,
) -> Vec<Stmt> {
    let subject = prune_expr(subject);
    // A `default` written between cases falls through into the case after it, which every
    // structural rewrite below (empty-case folding, constant materialization, the single-case
    // `if`) models as "default runs last". Such a switch only gets its bodies pruned; EIR
    // lowering places the bodies at their source positions.
    let default_is_middle = default
        .as_ref()
        .is_some_and(|body| !switch_default_runs_last(&cases, body));
    let cases: Vec<(Vec<Expr>, Vec<Stmt>)> = cases
        .into_iter()
        .map(|(patterns, body)| {
            (patterns.into_iter().map(prune_expr).collect(), prune_block(body))
        })
        .collect();
    let default = default.map(prune_block);
    if default_is_middle {
        return vec![Stmt {
            kind: StmtKind::Switch {
                subject,
                cases,
                default,
            },
            span,
            source_mode,
            strict_types,
            attributes: Vec::new(),
        }];
    }
    let (cases, default) = strip_final_switch_break(cases, default);
    let cases = normalize_switch_cases(drop_shadowed_switch_patterns(normalize_switch_cases(cases)));
    let default = normalize_optional_block(default);

    if cases.iter().all(|(_, body)| body.is_empty()) && default.is_none() {
        return expr_to_effect_stmt(subject);
    }

    // A STRAY `break` still targeting the SWITCH forbids every rewrite below: the single-case `if`
    // drops the switch, and the break would then target nothing — measured, it compiled to
    // `unreachable` and the program died with an illegal instruction where php answers a value.
    // A break ENDING a case body is not stray: it is that body's ordinary terminator and every
    // rewrite consumes it. Counting those refused nearly every switch there is.
    if switch_has_level_sensitive_loop_exit(&cases, &default)
        || switch_has_body_targeting_break(&cases, &default)
    {
        return vec![Stmt {
            kind: StmtKind::Switch {
                subject,
                cases,
                default,
            },
            span,
            source_mode,
            strict_types,
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
        if cases.len() == 1
            && !single_case_rewrite_would_clone_a_decision(&subject, &cases, &default)
        {
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
            source_mode,
            strict_types,
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
                    source_mode,
                    strict_types,
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

/// Drops the trailing `break` of the body that runs last in a `switch`: the `default` body when
/// there is one and it is last in source order, otherwise the last case body. Falling off that
/// body leaves the `switch` exactly as the `break` does, so the terminator only adds an extra
/// exit edge for later passes. A `default` written between cases keeps its `break` (falling off
/// it would enter the next case), and so does everything else in that switch. The strip happens
/// before case normalization so a case emptied this way is folded like any other empty trailing
/// case.
fn strip_final_switch_break(
    mut cases: Vec<(Vec<Expr>, Vec<Stmt>)>,
    default: Option<Vec<Stmt>>,
) -> (Vec<(Vec<Expr>, Vec<Stmt>)>, Option<Vec<Stmt>>) {
    if let Some(default_body) = default {
        if !switch_default_runs_last(&cases, &default_body) {
            return (cases, Some(default_body));
        }
        return (
            cases,
            Some(strip_trailing_terminator(default_body, TailTerminator::SwitchBreak)),
        );
    }
    if let Some((_, body)) = cases.last_mut() {
        *body = strip_trailing_terminator(std::mem::take(body), TailTerminator::SwitchBreak);
    }
    (cases, None)
}

/// Returns whether rewriting this single-case switch into an `if` would put a node the CHECKER
/// filed a local-binding decision against at TWO syntactic positions.
///
/// The decisions are keyed BY SPAN and a clone carries the original's span, so one decision would
/// then name two statements — the invariant `checker::binding_decision_ambiguity` certified on the
/// ORIGINAL program, and the one EIR lowering consults the maps under. This rewrite is the second
/// pass that can break it (DCE tail-sinking is the other, guarded by the same walker); it runs in
/// the normalize/prune phases, which is why `optimize::PostTypecheckOptimizer::prune` and
/// `::normalize` install the decision spans as well.
///
/// Two things the rewrite writes twice:
/// - the DEFAULT body — `materialize_switch_execution` appends it to the `then` branch when the
///   single case falls through, and emits it AGAIN as the whole `else` branch. It is the only body
///   that can be duplicated: the case body is materialized into `then` alone. It is checked
///   WHETHER OR NOT the case actually falls through into it, because deciding that here would mean
///   re-deriving `materialize_switch_execution`'s stop rule — and a rewrite this pass declines is
///   cheaper than a stop rule that drifts out of step with it.
/// - the SUBJECT expression — `build_switch_match_condition` clones it once per case pattern, so
///   more than one pattern means more than one copy. (Kill sites are filed against an EXPRESSION
///   span, which is why the subject is checked at all.)
///
/// A `true` answer costs the optimization on that one switch and nothing else.
fn single_case_rewrite_would_clone_a_decision(
    subject: &Expr,
    cases: &[(Vec<Expr>, Vec<Stmt>)],
    default: &Option<Vec<Stmt>>,
) -> bool {
    let default_body_carries_a_decision = default
        .as_ref()
        .is_some_and(|body| stmts_carry_local_binding_decision(body));
    let subject_is_cloned = cases
        .first()
        .is_some_and(|(patterns, _)| patterns.len() > 1);
    default_body_carries_a_decision
        || (subject_is_cloned && expr_carries_local_binding_decision(subject))
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
/// A `switch` case is decided by PHP's `==`, so this is exactly `loose_eq_values`: `case
/// true` matches any truthy subject (`switch (2)` selects it), `case null` matches `0` and
/// `""`, and PHP 8's string/number rules make `case 0` *not* match the subject `"foo"`.
/// Returns `None` only when the pair has no compile-time answer, which keeps the switch on
/// the runtime path.
pub(crate) fn compare_scalar_switch(left: &ScalarValue, right: &ScalarValue) -> Option<bool> {
    loose_eq_values(left, right)
}
