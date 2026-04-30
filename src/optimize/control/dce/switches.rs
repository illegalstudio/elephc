use super::*;
use super::guards::{
    extend_guards_for_switch_case,
    extend_guards_for_switch_case_no_match,
    extend_guards_for_switch_case_no_match_subject,
    guard_literal_truthy,
    has_excluded_guard,
    known_condition_value,
    scalar_guard_value,
};
use super::state::{GuardState, TailSinkTarget};
use super::tail::sink_tail_into_terminal_path;

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
                if guard_literal_truthy(&pattern_value) != subject_truthy {
                    continue;
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

        has_unknown = true;
    }

    if has_unknown {
        CaseMatch::Unknown
    } else {
        CaseMatch::NoMatch
    }
}

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

fn dce_switch_cases_with_guards(
    subject: &Expr,
    cases: Vec<(Vec<Expr>, Vec<Stmt>)>,
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

    for (patterns, body) in cases {
        let patterns: Vec<_> = patterns.into_iter().map(prune_expr).collect();
        let base_guards = if direct_only {
            &direct_entry_guards
        } else {
            guards
        };
        let case_guards = extend_guards_for_switch_case(subject, &patterns, base_guards);
        let body = trim_switch_noop_break(dce_block_with_guards(body, case_guards));
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

pub(super) fn dce_switch_stmt(
    subject: Expr,
    cases: Vec<(Vec<Expr>, Vec<Stmt>)>,
    default: Option<Vec<Stmt>>,
    span: crate::span::Span,
    guards: &GuardState,
) -> Vec<Stmt> {
    let subject = prune_expr(subject);
    let (cases, default) = prune_switch_patterns_with_guards(
        &subject,
        dce_switch_cases_with_guards(&subject, cases, guards),
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

pub(super) fn dce_switch_stmt_with_tail(
    subject: Expr,
    cases: Vec<(Vec<Expr>, Vec<Stmt>)>,
    default: Option<Vec<Stmt>>,
    tail: Vec<Stmt>,
    span: crate::span::Span,
    guards: &GuardState,
) -> Vec<Stmt> {
    let subject = prune_expr(subject);
    let tail = dce_block_with_guards(tail, guards.clone());
    let (cases, default) = prune_switch_patterns_with_guards(
        &subject,
        dce_switch_cases_with_guards(&subject, cases, guards),
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

    let no_default = default.is_none();
    let case_count = cases.len();
    for (index, (_, body)) in cases.iter_mut().enumerate() {
        match block_terminal_effect(body) {
            TerminalEffect::Breaks => {
                *body = sink_tail_into_terminal_path(
                    std::mem::take(body),
                    tail.clone(),
                    TailSinkTarget::Breaks,
                );
            }
            TerminalEffect::FallsThrough
                if no_default
                    && index + 1 == case_count
                    && matches!(reachability.case_tail_paths[index], TailPathKind::FallsThrough) =>
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

    dce_switch_stmt(subject, cases, default, span, guards)
}
