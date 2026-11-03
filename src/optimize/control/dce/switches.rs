//! Purpose:
//! Handles DCE switches cases.
//! Preserves observable effects while removing unreachable tails, redundant branches, or dead writes.
//!
//! Called from:
//! - `crate::optimize::control::dce`
//!
//! Key details:
//! - The pass must remain conservative around throws, finally blocks, switch fallthrough, method calls, and variable writes.

use super::*;
use super::guards::{
    extend_guards_for_switch_case,
    extend_guards_for_switch_case_no_match,
    extend_guards_for_switch_case_no_match_subject,
    guard_literal_truthy,
    has_excluded_guard,
    known_range_guard,
    known_condition_value,
    scalar_guard_value,
};
use super::state::{GuardLiteral, GuardState, TailSinkTarget};
use super::tail::sink_tail_into_terminal_path;

/// Classifies switch pattern matching against a known scalar subject value.
///
/// Returns `CaseMatch::Matches` if the subject value matches at least one pattern
/// and no pattern is unknown; `CaseMatch::NoMatch` if the subject value matches
/// no pattern and all patterns are known; `CaseMatch::Unknown` if any pattern
/// cannot be evaluated statically.
///
/// Uses loose switch comparison semantics (== instead of ===) and handles boolean
/// guard conditions by comparing the guard truthiness against the subject value.
fn classify_switch_patterns_for_exact_scalar(
    subject_value: &ScalarValue,
    patterns: &[Expr],
    guards: &GuardState,
) -> CaseMatch {
    let mut has_unknown = false;
    for pattern in patterns {
        if let Some(matches) = pattern_matches_scalar(subject_value, pattern, CaseComparison::LooseSwitch) {
            if matches {
                return CaseMatch::Matches;
            }
            continue;
        }

        if let ScalarValue::Bool(subject_bool) = subject_value {
            if let Some(pattern_bool) = known_condition_value(pattern, guards) {
                if pattern_bool == *subject_bool {
                    return CaseMatch::Matches;
                }
                continue;
            }
        }

        has_unknown = true;
    }

    if has_unknown {
        CaseMatch::Unknown
    } else {
        CaseMatch::NoMatch
    }
}

/// Classifies switch pattern matching for a variable subject with active guards.
///
/// Used when the subject is a Variable whose truthiness or scalar value can be
/// tracked through guard state. Returns `CaseMatch::Matches` if a pattern provably
/// matches, `CaseMatch::NoMatch` if all patterns provably don't match, or
/// `CaseMatch::Unknown` if matching cannot be determined statically.
///
/// Handles guard-based exclusions and truthiness comparisons for boolean patterns.
fn classify_switch_patterns_with_guards(
    subject: &Expr,
    patterns: &[Expr],
    guards: &GuardState,
) -> CaseMatch {
    if let Some(subject_value) = known_scalar_subject_value(subject, guards) {
        return classify_switch_patterns_for_exact_scalar(&subject_value, patterns, guards);
    }

    let ExprKind::Variable(name) = &subject.kind else {
        return CaseMatch::Unknown;
    };

    let mut has_unknown = false;
    for pattern in patterns {
        if let Some(subject_truthy) = known_subject_truthiness(subject, guards) {
            if let ExprKind::BoolLiteral(pattern_bool) = pattern.kind {
                if subject_truthy == pattern_bool {
                    return CaseMatch::Matches;
                }
                continue;
            }

            if let Some(pattern_value) = scalar_guard_value(pattern) {
                if let Some(pattern_truthy) = guard_literal_truthy(&pattern_value) {
                    if pattern_truthy != subject_truthy {
                        continue;
                    }
                }
            }
        }

        let Some(pattern_value) = scalar_guard_value(pattern) else {
            has_unknown = true;
            continue;
        };

        if has_excluded_guard(guards, name, &pattern_value) {
            continue;
        }

        if let (Some(interval), GuardLiteral::Int(pattern_int)) =
            (known_range_guard(guards, name), &pattern_value)
        {
            if !interval.contains(*pattern_int) {
                continue;
            }
        }

        has_unknown = true;
    }

    if has_unknown {
        CaseMatch::Unknown
    } else {
        CaseMatch::NoMatch
    }
}

/// Determines which switch cases are guaranteed to execute given known subject value and guards.
///
/// Returns a tuple of `(entry_case_indices, default_may_be_reached)` where:
/// - `entry_case_indices` are the case indices that may match based on static analysis
/// - `default_may_be_reached` indicates whether the default branch could still be reached
///
/// For boolean subjects, computes guards incrementally across cases. For other scalar
/// subjects, stops at the first guaranteed match. For variable subjects with guard state,
/// applies guard-based exclusions to rule out patterns.
pub(super) fn direct_switch_entry_blocks(
    subject: &Expr,
    cases: &[(Vec<Expr>, Vec<Stmt>)],
    has_default: bool,
    guards: &GuardState,
) -> (Vec<usize>, bool) {
    if let Some(subject_value) = known_scalar_subject_value(subject, guards) {
        if matches!(subject_value, ScalarValue::Bool(_)) {
            let mut no_match_guards = guards.clone();
            let mut entry_blocks = Vec::new();

            for (index, (patterns, _)) in cases.iter().enumerate() {
                match classify_switch_patterns_for_exact_scalar(&subject_value, patterns, &no_match_guards) {
                    CaseMatch::Matches => {
                        entry_blocks.push(index);
                        return (entry_blocks, false);
                    }
                    CaseMatch::Unknown => entry_blocks.push(index),
                    CaseMatch::NoMatch => {}
                }
                no_match_guards =
                    extend_guards_for_switch_case_no_match(&subject_value, patterns, &no_match_guards);
            }

            return (entry_blocks, has_default);
        }

        for (index, (patterns, _)) in cases.iter().enumerate() {
            match classify_switch_patterns_for_exact_scalar(&subject_value, patterns, guards) {
                CaseMatch::Matches => return (vec![index], false),
                CaseMatch::Unknown => return ((index..cases.len()).collect(), has_default),
                CaseMatch::NoMatch => {}
            }
        }
    }

    if matches!(subject.kind, ExprKind::Variable(_)) {
        let mut no_match_guards = guards.clone();
        let mut entry_blocks = Vec::new();
        for (index, (patterns, _)) in cases.iter().enumerate() {
            match classify_switch_patterns_with_guards(subject, patterns, &no_match_guards) {
                CaseMatch::Matches => {
                    entry_blocks.push(index);
                    return (entry_blocks, false);
                }
                CaseMatch::Unknown => entry_blocks.push(index),
                CaseMatch::NoMatch => {}
            }
            no_match_guards =
                extend_guards_for_switch_case_no_match_subject(subject, patterns, &no_match_guards);
        }
        return (entry_blocks, has_default);
    }

    ((0..cases.len()).collect(), has_default)
}

/// Removes switch cases and default branch that are unreachable based on guard state.
///
/// Builds a control-flow graph for the switch, collects reachable blocks from
/// guaranteed entry points, and filters out cases whose bodies are provably
/// unreachable. The default is dropped if its entry block is not reachable.
fn prune_unreachable_switch_blocks(
    subject: &Expr,
    cases: Vec<(Vec<Expr>, Vec<Stmt>)>,
    default: Option<Vec<Stmt>>,
    guards: &GuardState,
) -> (Vec<(Vec<Expr>, Vec<Stmt>)>, Option<Vec<Stmt>>) {
    let (direct_case_entries, direct_default_entry) =
        direct_switch_entry_blocks(subject, &cases, default.is_some(), guards);
    let cfg = build_switch_cfg(&cases, &default);
    let mut entry_blocks = direct_case_entries;
    if direct_default_entry {
        if let Some(default_entry) = cfg.default_entry {
            entry_blocks.push(default_entry);
        }
    }
    let reachable = collect_reachable_cfg_blocks(&cfg.blocks, &entry_blocks);
    let default_reachable = cfg.default_entry.is_some_and(|entry| reachable[entry]);
    let cases = cases
        .into_iter()
        .enumerate()
        .filter_map(|(index, case)| reachable[index].then_some(case))
        .collect();
    let default = if default_reachable { default } else { None };

    (cases, default)
}

/// Prunes unreachable switch cases and truncates the case list to only reachable entries.
///
/// First removes unreachable blocks via `prune_unreachable_switch_blocks`, then if the
/// subject has a known scalar value, identifies the first matching case and discards
/// all earlier cases (since they cannot be reached for this subject value). Returns
/// empty cases if no case is reachable.
fn prune_unreachable_switch_entries(
    subject: &Expr,
    cases: Vec<(Vec<Expr>, Vec<Stmt>)>,
    default: Option<Vec<Stmt>>,
    guards: &GuardState,
) -> (Vec<(Vec<Expr>, Vec<Stmt>)>, Option<Vec<Stmt>>) {
    let (cases, default) = prune_unreachable_switch_blocks(subject, cases, default, guards);
    if known_scalar_subject_value(subject, guards).is_none() {
        return (cases, default);
    }

    let (direct_case_entries, direct_default_entry) =
        direct_switch_entry_blocks(subject, &cases, default.is_some(), guards);
    if direct_case_entries.is_empty() {
        return (Vec::new(), direct_default_entry.then_some(()).and(default));
    }

    let first_entry = direct_case_entries[0];
    (cases[first_entry..].to_vec(), default)
}

/// Removes individual patterns from switch cases that provably don't match given guard state.
///
/// For each pattern in each case, determines whether it matches using guard-aware
/// classification. Patterns that provably don't match are dropped. If any pattern
/// in a case matches, direct entry optimization is disabled for all subsequent cases
/// (since control flow may have entered through an earlier case). The default branch
/// is dropped when direct entry is no longer possible.
fn prune_switch_patterns_with_guards(
    subject: &Expr,
    cases: Vec<(Vec<Expr>, Vec<Stmt>)>,
    default: Option<Vec<Stmt>>,
    guards: &GuardState,
) -> (Vec<(Vec<Expr>, Vec<Stmt>)>, Option<Vec<Stmt>>) {
    let mut no_match_guards = guards.clone();
    let mut direct_entries_possible = true;
    let mut pruned_cases = Vec::with_capacity(cases.len());

    for (patterns, body) in cases {
        if !direct_entries_possible {
            pruned_cases.push((Vec::new(), body));
            continue;
        }

        let mut kept_patterns = Vec::new();
        let mut local_no_match_guards = no_match_guards.clone();

        for pattern in patterns {
            match classify_switch_patterns_with_guards(
                subject,
                std::slice::from_ref(&pattern),
                &local_no_match_guards,
            ) {
                CaseMatch::Matches => {
                    kept_patterns.push(pattern);
                    direct_entries_possible = false;
                    break;
                }
                CaseMatch::Unknown => {
                    local_no_match_guards = extend_guards_for_switch_case_no_match_subject(
                        subject,
                        std::slice::from_ref(&pattern),
                        &local_no_match_guards,
                    );
                    kept_patterns.push(pattern);
                }
                CaseMatch::NoMatch => {
                    local_no_match_guards = extend_guards_for_switch_case_no_match_subject(
                        subject,
                        std::slice::from_ref(&pattern),
                        &local_no_match_guards,
                    );
                }
            }
        }

        if direct_entries_possible {
            no_match_guards = local_no_match_guards;
        }

        pruned_cases.push((kept_patterns, body));
    }

    let default = if direct_entries_possible { default } else { None };
    (pruned_cases, default)
}

/// Applies DCE to switch case bodies with guard state tracking across cases.
///
/// A direct-only case body receives the accumulated no-match guards plus its
/// matching case guard. Once a prior body can fall through, subsequent bodies
/// use only the outer guards because their case patterns were not evaluated on
/// every incoming path. Terminating cases preserve direct-only entry tracking.
/// A switch-noop break is trimmed only from the final case when no default can
/// receive fall-through; earlier breaks remain control-flow barriers.
fn dce_switch_cases_with_guards(
    subject: &Expr,
    cases: Vec<(Vec<Expr>, Vec<Stmt>)>,
    has_default: bool,
    guards: &GuardState,
) -> Vec<(Vec<Expr>, Vec<Stmt>)> {
    let trim_switch_noop_break = |body: Vec<Stmt>| {
        if body.len() == 1 && matches!(body[0].kind, StmtKind::Break(1)) {
            Vec::new()
        } else {
            body
        }
    };

    let mut direct_entry_guards = guards.clone();
    let mut direct_only = true;
    let mut processed = Vec::with_capacity(cases.len());
    let case_count = cases.len();

    for (index, (patterns, body)) in cases.into_iter().enumerate() {
        let patterns: Vec<_> = patterns.into_iter().map(prune_expr).collect();
        let case_guards = if direct_only {
            extend_guards_for_switch_case(subject, &patterns, &direct_entry_guards)
        } else {
            guards.clone()
        };
        let body = dce_block_with_guards(body, case_guards);
        let body = if !has_default && index + 1 == case_count {
            trim_switch_noop_break(body)
        } else {
            body
        };
        if direct_only {
            direct_entry_guards =
                extend_guards_for_switch_case_no_match_subject(subject, &patterns, &direct_entry_guards);
        }
        direct_only = direct_only
            && matches!(
                block_terminal_effect(&body),
                TerminalEffect::Breaks | TerminalEffect::ExitsCurrentBlock
            );
        processed.push((patterns, body));
    }

    processed
}

/// Applies DCE to a switch statement with optional default branch.
///
/// Normalizes and prunes patterns, removes unreachable cases, drops trailing
/// empty cases, and eliminates the switch entirely if all bodies are empty and
/// there is no default (replaced with subject expression as effect statement).
/// Preserves the switch if it contains level-sensitive loop exits that require
/// the full switch structure for correct semantics.
pub(super) fn dce_switch_stmt(
    subject: Expr,
    cases: Vec<(Vec<Expr>, Vec<Stmt>)>,
    default: Option<Vec<Stmt>>,
    span: crate::span::Span,
    guards: &GuardState,
) -> Vec<Stmt> {
    if switch_default_is_middle(&cases, &default) {
        return dce_switch_bodies_only(subject, cases, default, span, guards);
    }
    let subject = prune_expr(subject);
    let has_default = default.is_some();
    let (cases, default) = prune_switch_patterns_with_guards(
        &subject,
        dce_switch_cases_with_guards(&subject, cases, has_default, guards),
        default,
        guards,
    );
    let cases = normalize_switch_cases(drop_shadowed_switch_patterns(normalize_switch_cases(cases)));
    let (mut cases, default) = prune_unreachable_switch_entries(&subject, cases, default, guards);
    while cases.last().is_some_and(|(_, body)| body.is_empty()) {
        cases.pop();
    }
    let default = normalize_optional_block(default.map(|body| dce_block_with_guards(body, guards.clone())));

    if switch_has_level_sensitive_loop_exit(&cases, &default) {
        return vec![Stmt::new(
            StmtKind::Switch {
                subject,
                cases,
                default,
            },
            span,
        )];
    }

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

    vec![Stmt::new(
        StmtKind::Switch {
            subject,
            cases,
            default,
        },
        span,
    )]
}

/// Returns whether the switch has a `default` written between cases, which falls through into
/// the case after it. Every switch rewrite in this pass models `default` as the body that runs
/// last, so such a switch must not be reshaped.
fn switch_default_is_middle(cases: &[(Vec<Expr>, Vec<Stmt>)], default: &Option<Vec<Stmt>>) -> bool {
    default
        .as_ref()
        .is_some_and(|body| !switch_default_runs_last(cases, body))
}

/// Applies DCE to each body of a switch whose `default` sits between cases, under the outer
/// guards only, and keeps the switch shape untouched so EIR lowering can place the bodies at
/// their source positions. No case pruning, tail sinking, or guard fact is derived from it.
fn dce_switch_bodies_only(
    subject: Expr,
    cases: Vec<(Vec<Expr>, Vec<Stmt>)>,
    default: Option<Vec<Stmt>>,
    span: crate::span::Span,
    guards: &GuardState,
) -> Vec<Stmt> {
    let cases = cases
        .into_iter()
        .map(|(patterns, body)| {
            (
                patterns.into_iter().map(prune_expr).collect(),
                dce_block_with_guards(body, guards.clone()),
            )
        })
        .collect();
    let default = default.map(|body| dce_block_with_guards(body, guards.clone()));
    vec![Stmt::new(
        StmtKind::Switch {
            subject: prune_expr(subject),
            cases,
            default,
        },
        span,
    )]
}

/// Applies DCE to a switch statement with a tail of statements to execute after the switch.
///
/// The tail is first processed with DCE. If the switch has level-sensitive loop exits, has no
/// `default` (the no-match path would need a copy of its own), or the tail carries a loop exit,
/// the tail is appended after the full switch statement. If the switch has unknown
/// reachability paths, the tail is appended to the switch result without further
/// optimization. Otherwise, the tail is sunk into case and default bodies that
/// terminate with `break` or `falls through`, using `sink_tail_into_terminal_path`.
/// Returns the result of `dce_switch_stmt` on the processed switch.
pub(super) fn dce_switch_stmt_with_tail(
    subject: Expr,
    cases: Vec<(Vec<Expr>, Vec<Stmt>)>,
    default: Option<Vec<Stmt>>,
    tail: Vec<Stmt>,
    span: crate::span::Span,
    guards: &GuardState,
) -> Vec<Stmt> {
    if switch_default_is_middle(&cases, &default) {
        let mut stmts = dce_switch_bodies_only(subject, cases, default, span, guards);
        stmts.extend(dce_block_with_guards(tail, guards.clone()));
        return stmts;
    }
    let subject = prune_expr(subject);
    let tail = dce_block_with_guards(tail, guards.clone());
    let has_default = default.is_some();
    let (cases, default) = prune_switch_patterns_with_guards(
        &subject,
        dce_switch_cases_with_guards(&subject, cases, has_default, guards),
        default,
        guards,
    );
    let cases = normalize_switch_cases(drop_shadowed_switch_patterns(normalize_switch_cases(cases)));
    let (cases, default) = prune_unreachable_switch_entries(&subject, cases, default, guards);
    let mut cases = cases;
    while cases.last().is_some_and(|(_, body)| body.is_empty()) {
        cases.pop();
    }
    let mut default = normalize_optional_block(default.map(|body| dce_block_with_guards(body, guards.clone())));

    if switch_has_level_sensitive_loop_exit(&cases, &default) {
        let mut stmts = dce_switch_stmt(subject, cases, default, span, guards);
        stmts.extend(tail);
        return stmts;
    }

    if tail.is_empty() {
        return dce_switch_stmt(subject, cases, default, span, guards);
    }

    // Without a `default`, the tail is reached by every `break` path, by the last case falling
    // off the switch, AND by a subject that matches nothing. Sinking would clone it once per
    // `break` plus once into a synthesized `default` for the last two paths — a default that is
    // dead when the cases are exhaustive — so the tail simply stays after the switch.
    //
    // A `break` / `continue` in the tail targets a loop around the switch; inside a case or
    // default body the same statement would target the switch itself, so such a tail stays
    // after the switch as well.
    if default.is_none() || block_contains_loop_exit(&tail) {
        let mut stmts = dce_switch_stmt(subject, cases, default, span, guards);
        stmts.extend(tail);
        return stmts;
    }

    let reachability = analyze_switch_tail_paths(&cases, &default);
    if reachability
        .case_tail_paths
        .iter()
        .copied()
        .chain(reachability.default_tail_path)
        .any(|path| matches!(path, TailPathKind::Unknown))
    {
        let mut stmts = dce_switch_stmt(subject, cases, default, span, guards);
        stmts.extend(tail);
        return stmts;
    }

    if let Some(body) = default.as_mut() {
        match block_terminal_effect(body) {
            TerminalEffect::Breaks => {
                *body = sink_tail_into_terminal_path(
                    std::mem::take(body),
                    tail.clone(),
                    TailSinkTarget::Breaks,
                );
            }
            TerminalEffect::FallsThrough
                if matches!(reachability.default_tail_path, Some(TailPathKind::FallsThrough)) =>
            {
                *body = sink_tail_into_terminal_path(
                    std::mem::take(body),
                    tail.clone(),
                    TailSinkTarget::FallsThrough,
                );
            }
            _ => {}
        }
    }

    for (_, body) in cases.iter_mut() {
        if matches!(block_terminal_effect(body), TerminalEffect::Breaks) {
            *body = sink_tail_into_terminal_path(
                std::mem::take(body),
                tail.clone(),
                TailSinkTarget::Breaks,
            );
        }
    }

    dce_switch_stmt(subject, cases, default, span, guards)
}
