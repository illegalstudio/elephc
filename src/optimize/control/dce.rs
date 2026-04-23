use super::*;

#[derive(Clone, Copy)]
enum TailSinkTarget {
    FallsThrough,
    Breaks,
}

#[derive(Clone, Default)]
struct GuardState {
    truthy_vars: Vec<String>,
    falsy_vars: Vec<String>,
    bool_true_vars: Vec<String>,
    bool_false_vars: Vec<String>,
    exact_guards: Vec<ExactGuard>,
    excluded_guards: Vec<ExactGuard>,
    condition_guards: Vec<ConditionGuard>,
}

#[derive(Clone, PartialEq, Eq)]
struct ExactGuard {
    name: String,
    value: GuardLiteral,
}

#[derive(Clone)]
struct ConditionGuard {
    condition: Expr,
    value: bool,
    names: Vec<String>,
}

#[derive(Clone, PartialEq, Eq)]
enum GuardLiteral {
    Bool(bool),
    Null,
    Int(i64),
    Float(u64),
    String(String),
}

pub(crate) fn dce_block(body: Vec<Stmt>) -> Vec<Stmt> {
    dce_block_with_guards(body, GuardState::default())
}

fn dce_block_with_guards(body: Vec<Stmt>, mut guards: GuardState) -> Vec<Stmt> {
    let mut eliminated = Vec::new();
    let mut stmts = body.into_iter().peekable();
    while let Some(stmt) = stmts.next() {
        let has_tail = stmts.peek().is_some();
        let use_tail_sink = has_tail
            && matches!(
                stmt.kind,
                StmtKind::If { .. } | StmtKind::IfDef { .. } | StmtKind::Switch { .. } | StmtKind::Try { .. }
            );
        let dce_stmt = if use_tail_sink {
            let tail: Vec<Stmt> = stmts.clone().collect();
            dce_stmt_with_tail(stmt, tail, &guards)
        } else {
            dce_stmt_with_guards(stmt, &guards)
        };
        let stops_here = dce_stmt
            .last()
            .is_some_and(|stmt| !matches!(stmt_terminal_effect(stmt), TerminalEffect::FallsThrough));
        for stmt in &dce_stmt {
            invalidate_guards_for_stmt(stmt, &mut guards);
        }
        eliminated.extend(dce_stmt);
        if stops_here {
            break;
        }
        if use_tail_sink {
            break;
        }
    }
    eliminated
}

fn append_tail_to_fallthrough_path(mut body: Vec<Stmt>, tail: Vec<Stmt>) -> Vec<Stmt> {
    if block_reaches_following_stmt(&body) {
        body.extend(tail);
    }
    body
}

fn block_matches_tail_target(body: &[Stmt], target: TailSinkTarget) -> bool {
    matches!(
        (block_terminal_effect(body), target),
        (TerminalEffect::FallsThrough, TailSinkTarget::FallsThrough)
            | (TerminalEffect::Breaks, TailSinkTarget::Breaks)
    )
}

fn sink_tail_into_terminal_path(
    mut body: Vec<Stmt>,
    tail: Vec<Stmt>,
    target: TailSinkTarget,
) -> Vec<Stmt> {
    let Some(stmt) = body.pop() else {
        return tail;
    };

    let rewritten = sink_tail_into_terminal_stmt(stmt, tail, target);
    body.extend(rewritten);
    body
}

fn sink_tail_into_terminal_stmt(stmt: Stmt, tail: Vec<Stmt>, target: TailSinkTarget) -> Vec<Stmt> {
    let span = stmt.span;
    match stmt.kind {
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => {
            let rewrite_branch = |body: Vec<Stmt>, target: TailSinkTarget, tail: &Vec<Stmt>| {
                if block_matches_tail_target(&body, target) {
                    sink_tail_into_terminal_path(body, tail.clone(), target)
                } else {
                    body
                }
            };
            let then_body = rewrite_branch(then_body, target, &tail);
            let elseif_clauses: Vec<_> = elseif_clauses
                .into_iter()
                .map(|(condition, body)| (condition, rewrite_branch(body, target, &tail)))
                .collect();
            let else_body = else_body.map(|body| rewrite_branch(body, target, &tail));
            vec![Stmt::new(
                StmtKind::If {
                    condition,
                    then_body,
                    elseif_clauses,
                    else_body,
                },
                span,
            )]
        }
        StmtKind::IfDef {
            symbol,
            then_body,
            else_body,
        } => {
            let then_body = if block_matches_tail_target(&then_body, target) {
                sink_tail_into_terminal_path(then_body, tail.clone(), target)
            } else {
                then_body
            };
            let else_body = else_body.map(|body| {
                if block_matches_tail_target(&body, target) {
                    sink_tail_into_terminal_path(body, tail.clone(), target)
                } else {
                    body
                }
            });
            vec![Stmt::new(
                StmtKind::IfDef {
                    symbol,
                    then_body,
                    else_body,
                },
                span,
            )]
        }
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            let try_body = if block_matches_tail_target(&try_body, target) {
                sink_tail_into_terminal_path(try_body, tail.clone(), target)
            } else {
                try_body
            };
            let catches = catches
                .into_iter()
                .map(|catch| crate::parser::ast::CatchClause {
                    body: if block_matches_tail_target(&catch.body, target) {
                        sink_tail_into_terminal_path(catch.body, tail.clone(), target)
                    } else {
                        catch.body
                    },
                    ..catch
                })
                .collect();
            vec![Stmt::new(
                StmtKind::Try {
                    try_body,
                    catches,
                    finally_body,
                },
                span,
            )]
        }
        _ if matches!(target, TailSinkTarget::FallsThrough)
            && matches!(stmt_terminal_effect(&stmt), TerminalEffect::FallsThrough) =>
        {
            let mut stmts = vec![stmt];
            stmts.extend(tail);
            stmts
        }
        StmtKind::Break if matches!(target, TailSinkTarget::Breaks) => {
            let mut stmts = tail;
            if block_reaches_following_stmt(&stmts) {
                stmts.push(Stmt::new(StmtKind::Break, span));
            }
            stmts
        }
        _ => vec![stmt],
    }
}

fn dce_if_tail(
    mut elseif_clauses: Vec<(Expr, Vec<Stmt>)>,
    else_body: Option<Vec<Stmt>>,
    span: crate::span::Span,
) -> Vec<Stmt> {
    let Some((condition, body)) = elseif_clauses.first().cloned() else {
        return else_body.unwrap_or_default();
    };
    elseif_clauses.remove(0);
    let rest = dce_if_tail(elseif_clauses, else_body, span);

    if body.is_empty() {
        if rest.is_empty() {
            expr_to_effect_stmt(condition)
        } else {
            vec![build_if_stmt(
                invert_condition(condition),
                rest,
                Vec::new(),
                None,
                span,
            )]
        }
    } else {
        vec![build_if_stmt(
            condition,
            body,
            Vec::new(),
            normalize_optional_block(Some(rest)),
            span,
        )]
    }
}

fn dce_if_stmt(
    condition: Expr,
    then_body: Vec<Stmt>,
    elseif_clauses: Vec<(Expr, Vec<Stmt>)>,
    else_body: Option<Vec<Stmt>>,
    span: crate::span::Span,
    guards: &GuardState,
) -> Vec<Stmt> {
    let condition = prune_expr(condition);
    if let Some(true) = known_condition_value(&condition, guards) {
        return dce_block_with_guards(then_body, extend_guards(guards, &condition, true));
    }

    if let Some(false) = known_condition_value(&condition, guards) {
        return dce_if_false_path(condition, elseif_clauses, else_body, span, guards);
    }

    let then_body = dce_block_with_guards(then_body, extend_guards(guards, &condition, true));
    let mut false_guards = extend_guards(guards, &condition, false);
    let mut processed_elseif_clauses = Vec::with_capacity(elseif_clauses.len());
    for (condition, body) in elseif_clauses.into_iter() {
        let condition = prune_expr(condition);
        let body = dce_block_with_guards(body, extend_guards(&false_guards, &condition, true));
        false_guards = extend_guards(&false_guards, &condition, false);
        processed_elseif_clauses.push((condition, body));
    }
    let else_body =
        normalize_optional_block(else_body.map(|body| dce_block_with_guards(body, false_guards)));
    let (condition, then_body, elseif_clauses, else_body) =
        prune_unreachable_if_entries(condition, then_body, processed_elseif_clauses, else_body, guards);
    if matches!(condition.kind, ExprKind::BoolLiteral(false))
        && then_body.is_empty()
        && elseif_clauses.is_empty()
    {
        return else_body.unwrap_or_default();
    }
    let tail = dce_if_tail(elseif_clauses.clone(), else_body.clone(), span);

    if tail.is_empty() {
        if then_body.is_empty() {
            return expr_to_effect_stmt(condition);
        }

        return vec![build_if_stmt(
            condition,
            then_body,
            Vec::new(),
            None,
            span,
        )];
    }

    if elseif_clauses.is_empty() {
        if then_body.is_empty() && else_body.is_none() {
            return expr_to_effect_stmt(condition);
        }

        if then_body.is_empty() {
            if let Some(else_body) = else_body {
                return vec![build_if_stmt(
                    invert_condition(condition),
                    else_body,
                    Vec::new(),
                    None,
                    span,
                )];
            }
        }

        if tail == then_body {
            let mut stmts = expr_to_effect_stmt(condition);
            stmts.extend(then_body);
            return stmts;
        }
    }

    if then_body.is_empty() {
        return vec![build_if_stmt(
            invert_condition(condition),
            tail,
            Vec::new(),
            None,
            span,
        )];
    }

    vec![Stmt::new(
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses: Vec::new(),
            else_body: normalize_optional_block(Some(tail)),
        },
        span,
    )]
}

fn direct_if_entry_blocks(
    condition: &Expr,
    elseif_clauses: &[(Expr, Vec<Stmt>)],
    has_else: bool,
    guards: &GuardState,
) -> (Vec<usize>, bool) {
    let mut false_guards = extend_guards(guards, condition, false);
    let mut entry_blocks = Vec::new();

    match known_condition_value(condition, guards) {
        Some(true) => return (vec![0], false),
        Some(false) => {}
        None => entry_blocks.push(0),
    }

    for (index, (condition, _)) in elseif_clauses.iter().enumerate() {
        match known_condition_value(condition, &false_guards) {
            Some(true) => return (entry_blocks.into_iter().chain(std::iter::once(index + 1)).collect(), false),
            Some(false) => {}
            None => entry_blocks.push(index + 1),
        }
        false_guards = extend_guards(&false_guards, condition, false);
    }

    (entry_blocks, has_else)
}

fn prune_unreachable_if_entries(
    condition: Expr,
    then_body: Vec<Stmt>,
    elseif_clauses: Vec<(Expr, Vec<Stmt>)>,
    else_body: Option<Vec<Stmt>>,
    guards: &GuardState,
) -> (Expr, Vec<Stmt>, Vec<(Expr, Vec<Stmt>)>, Option<Vec<Stmt>>) {
    let cfg = build_if_cfg(&then_body, &elseif_clauses, &else_body);
    let (entry_branches, else_reachable) =
        direct_if_entry_blocks(&condition, &elseif_clauses, else_body.is_some(), guards);
    let mut entry_blocks: Vec<_> = entry_branches
        .into_iter()
        .filter_map(|index| cfg.body_entries.get(index).copied())
        .collect();
    if else_reachable {
        if let Some(else_entry) = cfg.else_entry {
            entry_blocks.push(else_entry);
        }
    }

    let reachable = collect_reachable_cfg_blocks(&cfg.blocks, &entry_blocks);
    let mut remaining_clauses = Vec::new();
    if reachable
        .get(cfg.body_entries[0])
        .copied()
        .unwrap_or_default()
    {
        remaining_clauses.push((condition, then_body));
    }
    for ((condition, body), &entry) in elseif_clauses.into_iter().zip(cfg.body_entries.iter().skip(1)) {
        if reachable.get(entry).copied().unwrap_or_default() {
            remaining_clauses.push((condition, body));
        }
    }

    let else_body = else_body.filter(|_| {
        cfg.else_entry
            .and_then(|entry| reachable.get(entry))
            .copied()
            .unwrap_or(false)
    });

    if remaining_clauses.is_empty() {
        return (
            Expr::new(ExprKind::BoolLiteral(false), crate::span::Span::dummy()),
            Vec::new(),
            Vec::new(),
            else_body,
        );
    }

    let (condition, then_body) = remaining_clauses.remove(0);
    (condition, then_body, remaining_clauses, else_body)
}

fn dce_if_false_path(
    condition: Expr,
    mut elseif_clauses: Vec<(Expr, Vec<Stmt>)>,
    else_body: Option<Vec<Stmt>>,
    span: crate::span::Span,
    guards: &GuardState,
) -> Vec<Stmt> {
    let false_guards = extend_guards(guards, &condition, false);
    if let Some((condition, body)) = elseif_clauses.first().cloned() {
        elseif_clauses.remove(0);
        dce_if_stmt(condition, body, elseif_clauses, else_body, span, &false_guards)
    } else {
        else_body
            .map(|body| dce_block_with_guards(body, false_guards))
            .unwrap_or_default()
    }
}

fn guard_literal_to_scalar(value: &GuardLiteral) -> ScalarValue {
    match value {
        GuardLiteral::Bool(value) => ScalarValue::Bool(*value),
        GuardLiteral::Null => ScalarValue::Null,
        GuardLiteral::Int(value) => ScalarValue::Int(*value),
        GuardLiteral::Float(bits) => ScalarValue::Float(f64::from_bits(*bits)),
        GuardLiteral::String(value) => ScalarValue::String(value.clone()),
    }
}

fn known_scalar_subject_value(subject: &Expr, guards: &GuardState) -> Option<ScalarValue> {
    scalar_value(subject).or_else(|| match &subject.kind {
        ExprKind::Variable(name) => known_exact_guard(guards, name).map(guard_literal_to_scalar),
        _ => None,
    })
}

fn known_subject_truthiness(subject: &Expr, guards: &GuardState) -> Option<bool> {
    if let Some(subject_value) = known_scalar_subject_value(subject, guards) {
        let guard_literal = match subject_value {
            ScalarValue::Bool(value) => GuardLiteral::Bool(value),
            ScalarValue::Null => GuardLiteral::Null,
            ScalarValue::Int(value) => GuardLiteral::Int(value),
            ScalarValue::Float(value) => GuardLiteral::Float(value.to_bits()),
            ScalarValue::String(value) => GuardLiteral::String(value),
        };
        return Some(guard_literal_truthy(&guard_literal));
    }

    let ExprKind::Variable(name) = &subject.kind else {
        return None;
    };

    if guards.bool_true_vars.iter().any(|known| known == name)
        || guards.truthy_vars.iter().any(|known| known == name)
    {
        return Some(true);
    }

    if guards.bool_false_vars.iter().any(|known| known == name)
        || guards.falsy_vars.iter().any(|known| known == name)
    {
        return Some(false);
    }

    None
}

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

fn direct_switch_entry_blocks(
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
        if body.len() == 1 && matches!(body[0].kind, StmtKind::Break) {
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

fn dce_switch_stmt(
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

fn dce_switch_stmt_with_tail(
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

fn dce_try_stmt(
    try_body: Vec<Stmt>,
    catches: Vec<crate::parser::ast::CatchClause>,
    finally_body: Option<Vec<Stmt>>,
    span: crate::span::Span,
    guards: &GuardState,
) -> Vec<Stmt> {
    let try_body = dce_block_with_guards(try_body, guards.clone());
    let catch_guards = invalidated_guards_for_throw_paths(guards, &try_body);
    let catches: Vec<_> = catches
        .into_iter()
        .map(|catch| {
            let mut body_guards = catch_guards.clone();
            if let Some(variable) = catch.variable.as_deref() {
                clear_guards_for_name(&mut body_guards, variable);
            }
            crate::parser::ast::CatchClause {
                exception_types: catch.exception_types,
                variable: catch.variable,
                body: dce_block_with_guards(catch.body, body_guards),
            }
        })
        .collect();
    let catches = if block_may_throw(&try_body) {
        normalize_catch_clauses(drop_shadowed_catch_clauses(normalize_catch_clauses(catches)))
    } else {
        Vec::new()
    };
    let finally_guards = invalidated_guards_for_finally_paths(guards, &try_body, &catches);
    let finally_body =
        normalize_optional_block(finally_body.map(|body| dce_block_with_guards(body, finally_guards)));

    if try_body.is_empty() {
        return finally_body.unwrap_or_default();
    }

    if catches.is_empty() && finally_body.is_none() {
        return try_body;
    }

    if catches.is_empty() {
        if let Some(finally_body) = finally_body {
            if !block_may_throw(&try_body)
                && matches!(block_terminal_effect(&try_body), TerminalEffect::FallsThrough)
            {
                let mut stmts = try_body;
                stmts.extend(finally_body);
                return stmts;
            }

            return vec![Stmt::new(
                StmtKind::Try {
                    try_body,
                    catches: Vec::new(),
                    finally_body: Some(finally_body),
                },
                span,
            )];
        }
    }

    vec![Stmt::new(
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        },
        span,
    )]
}

fn dce_try_stmt_with_tail(
    try_body: Vec<Stmt>,
    catches: Vec<crate::parser::ast::CatchClause>,
    finally_body: Option<Vec<Stmt>>,
    tail: Vec<Stmt>,
    span: crate::span::Span,
    guards: &GuardState,
) -> Vec<Stmt> {
    let try_body = dce_block_with_guards(try_body, guards.clone());
    let catch_guards = invalidated_guards_for_throw_paths(guards, &try_body);
    let catches: Vec<_> = catches
        .into_iter()
        .map(|catch| {
            let mut body_guards = catch_guards.clone();
            if let Some(variable) = catch.variable.as_deref() {
                clear_guards_for_name(&mut body_guards, variable);
            }
            crate::parser::ast::CatchClause {
                exception_types: catch.exception_types,
                variable: catch.variable,
                body: dce_block_with_guards(catch.body, body_guards),
            }
        })
        .collect();
    let catches = if block_may_throw(&try_body) {
        normalize_catch_clauses(drop_shadowed_catch_clauses(normalize_catch_clauses(catches)))
    } else {
        Vec::new()
    };
    let finally_guards = invalidated_guards_for_finally_paths(guards, &try_body, &catches);
    let finally_body =
        normalize_optional_block(finally_body.map(|body| dce_block_with_guards(body, finally_guards)));
    let tail = dce_block_with_guards(tail, guards.clone());

    if tail.is_empty() {
        return dce_try_stmt(try_body, catches, finally_body, span, guards);
    }

    let reachability = analyze_try_tail_paths(&try_body, &catches, &finally_body);

    if finally_body.is_none() {
        if matches!(reachability.try_tail_path, TailPathKind::FallsThrough)
            || reachability
                .catch_tail_paths
                .iter()
                .any(|path| matches!(path, TailPathKind::FallsThrough))
        {
            let try_body = append_tail_to_fallthrough_path(try_body, tail.clone());
            let catches = catches
                .into_iter()
                .zip(reachability.catch_tail_paths)
                .map(|catch| crate::parser::ast::CatchClause {
                    body: if matches!(catch.1, TailPathKind::FallsThrough) {
                        append_tail_to_fallthrough_path(catch.0.body, tail.clone())
                    } else {
                        catch.0.body
                    },
                    ..catch.0
                })
                .collect();
            return dce_try_stmt(try_body, catches, finally_body, span, guards);
        }
    }

    if reachability.can_sink_into_finally {
        let finally_body =
            normalize_optional_block(finally_body.map(|body| append_tail_to_fallthrough_path(body, tail)));
        return dce_try_stmt(try_body, catches, finally_body, span, guards);
    }

    let mut stmts = dce_try_stmt(try_body, catches, finally_body, span, guards);
    if stmts
        .last()
        .is_some_and(|stmt| matches!(stmt_terminal_effect(stmt), TerminalEffect::FallsThrough))
    {
        stmts.extend(tail);
    }
    stmts
}

fn dce_stmt_with_tail(stmt: Stmt, tail: Vec<Stmt>, guards: &GuardState) -> Vec<Stmt> {
    let span = stmt.span;
    match stmt.kind {
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => {
            let reachability = analyze_if_tail_paths(&then_body, &elseif_clauses, &else_body);
            let then_body = if reachability.then_sinks_tail {
                append_tail_to_fallthrough_path(then_body, tail.clone())
            } else {
                then_body
            };
            let elseif_clauses: Vec<_> = elseif_clauses
                .into_iter()
                .zip(reachability.elseif_sinks_tail)
                .map(|((condition, body), sinks_tail)| {
                    let body = if sinks_tail {
                        append_tail_to_fallthrough_path(body, tail.clone())
                    } else {
                        body
                    };
                    (condition, body)
                })
                .collect();
            let else_body = match else_body {
                Some(body) if reachability.else_sinks_tail => Some(append_tail_to_fallthrough_path(body, tail)),
                Some(body) => Some(body),
                None if reachability.implicit_else_sinks_tail => Some(tail),
                None => None,
            };
            dce_if_stmt(condition, then_body, elseif_clauses, else_body, span, guards)
        }
        StmtKind::IfDef {
            symbol,
            then_body,
            else_body,
        } => {
            let reachability = analyze_ifdef_tail_paths(&then_body, &else_body);
            let then_body = if reachability.then_sinks_tail {
                append_tail_to_fallthrough_path(then_body, tail.clone())
            } else {
                then_body
            };
            let else_body = match else_body {
                Some(body) if reachability.else_sinks_tail => Some(append_tail_to_fallthrough_path(body, tail)),
                Some(body) => Some(body),
                None if reachability.implicit_else_sinks_tail => Some(tail),
                None => None,
            };
            dce_stmt_with_guards(Stmt::new(
                StmtKind::IfDef {
                    symbol,
                    then_body,
                    else_body,
                },
                span,
            ), guards)
        }
        StmtKind::Switch {
            subject,
            cases,
            default,
        } => dce_switch_stmt_with_tail(subject, cases, default, tail, span, guards),
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => dce_try_stmt_with_tail(try_body, catches, finally_body, tail, span, guards),
        _ => {
            let mut stmts = dce_stmt_with_guards(stmt, guards);
            if stmts
                .last()
                .is_some_and(|stmt| matches!(stmt_terminal_effect(stmt), TerminalEffect::FallsThrough))
            {
                stmts.extend(dce_block_with_guards(tail, guards.clone()));
            }
            stmts
        }
    }
}

pub(crate) fn dce_stmt(stmt: Stmt) -> Vec<Stmt> {
    dce_stmt_with_guards(stmt, &GuardState::default())
}

fn dce_stmt_with_guards(stmt: Stmt, guards: &GuardState) -> Vec<Stmt> {
    let span = stmt.span;
    match stmt.kind {
        StmtKind::Echo(expr) => vec![Stmt {
            kind: StmtKind::Echo(prune_expr(expr)),
            span,
        }],
        StmtKind::Assign { name, value } => vec![Stmt {
            kind: StmtKind::Assign {
                name,
                value: prune_expr(value),
            },
            span,
        }],
        StmtKind::TypedAssign {
            name,
            type_expr,
            value,
        } => vec![Stmt {
            kind: StmtKind::TypedAssign {
                name,
                type_expr,
                value: prune_expr(value),
            },
            span,
        }],
        StmtKind::PropertyAssign {
            object,
            property,
            value,
        } => vec![Stmt {
            kind: StmtKind::PropertyAssign {
                object: Box::new(prune_expr(*object)),
                property,
                value: prune_expr(value),
            },
            span,
        }],
        StmtKind::PropertyArrayAssign {
            object,
            property,
            index,
            value,
        } => vec![Stmt {
            kind: StmtKind::PropertyArrayAssign {
                object: Box::new(prune_expr(*object)),
                property,
                index: prune_expr(index),
                value: prune_expr(value),
            },
            span,
        }],
        StmtKind::PropertyArrayPush {
            object,
            property,
            value,
        } => vec![Stmt {
            kind: StmtKind::PropertyArrayPush {
                object: Box::new(prune_expr(*object)),
                property,
                value: prune_expr(value),
            },
            span,
        }],
        StmtKind::ArrayAssign { array, index, value } => vec![Stmt {
            kind: StmtKind::ArrayAssign {
                array,
                index: prune_expr(index),
                value: prune_expr(value),
            },
            span,
        }],
        StmtKind::ArrayPush { array, value } => vec![Stmt {
            kind: StmtKind::ArrayPush {
                array,
                value: prune_expr(value),
            },
            span,
        }],
        StmtKind::ListUnpack { vars, value } => vec![Stmt {
            kind: StmtKind::ListUnpack {
                vars,
                value: prune_expr(value),
            },
            span,
        }],
        StmtKind::StaticVar { name, init } => vec![Stmt {
            kind: StmtKind::StaticVar {
                name,
                init: prune_expr(init),
            },
            span,
        }],
        StmtKind::ConstDecl { name, value } => vec![Stmt {
            kind: StmtKind::ConstDecl {
                name,
                value: prune_expr(value),
            },
            span,
        }],
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => dce_if_stmt(condition, then_body, elseif_clauses, else_body, span, guards),
        StmtKind::IfDef {
            symbol,
            then_body,
            else_body,
        } => {
            let then_body = dce_block_with_guards(then_body, guards.clone());
            let else_body =
                normalize_optional_block(else_body.map(|body| dce_block_with_guards(body, guards.clone())));
            if then_body.is_empty() && else_body.is_none() {
                Vec::new()
            } else {
                vec![Stmt {
                    kind: StmtKind::IfDef {
                        symbol,
                        then_body,
                        else_body,
                    },
                    span,
                }]
            }
        }
        StmtKind::While { condition, body } => vec![Stmt {
            kind: StmtKind::While {
                condition: prune_expr(condition),
                body: dce_block_with_guards(body, guards.clone()),
            },
            span,
        }],
        StmtKind::DoWhile { body, condition } => vec![Stmt {
            kind: StmtKind::DoWhile {
                body: dce_block_with_guards(body, guards.clone()),
                condition: prune_expr(condition),
            },
            span,
        }],
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => vec![Stmt {
            kind: StmtKind::For {
                init: init.and_then(|stmt| dce_stmt(*stmt).into_iter().next().map(Box::new)),
                condition: condition.map(prune_expr),
                update: update.and_then(|stmt| dce_stmt(*stmt).into_iter().next().map(Box::new)),
                body: dce_block_with_guards(body, guards.clone()),
            },
            span,
        }],
        StmtKind::Foreach {
            array,
            key_var,
            value_var,
            body,
        } => vec![Stmt {
            kind: StmtKind::Foreach {
                array: prune_expr(array),
                key_var,
                value_var,
                body: dce_block_with_guards(body, guards.clone()),
            },
            span,
        }],
        StmtKind::Switch {
            subject,
            cases,
            default,
        } => dce_switch_stmt(subject, cases, default, span, guards),
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => dce_try_stmt(try_body, catches, finally_body, span, guards),
        StmtKind::NamespaceBlock { name, body } => vec![Stmt {
            kind: StmtKind::NamespaceBlock {
                name,
                body: dce_block_with_guards(body, guards.clone()),
            },
            span,
        }],
        StmtKind::FunctionDecl {
            name,
            params,
            variadic,
            return_type,
            body,
        } => vec![Stmt {
            kind: StmtKind::FunctionDecl {
                name,
                params,
                variadic,
                return_type,
                body: dce_block_with_guards(body, GuardState::default()),
            },
            span,
        }],
        StmtKind::Return(expr) => vec![Stmt {
            kind: StmtKind::Return(expr.map(prune_expr)),
            span,
        }],
        StmtKind::Throw(expr) => vec![Stmt {
            kind: StmtKind::Throw(prune_expr(expr)),
            span,
        }],
        StmtKind::ClassDecl {
            name,
            extends,
            implements,
            is_abstract,
            is_readonly_class,
            trait_uses,
            properties,
            methods,
        } => {
            let parent_name = extends.as_ref().map(|parent| parent.as_str().to_string());
            let methods = methods
                .into_iter()
                .map(|method| dce_method(method, &name, parent_name.as_deref()))
                .collect();
            vec![Stmt {
                kind: StmtKind::ClassDecl {
                    name,
                    extends,
                    implements,
                    is_abstract,
                    is_readonly_class,
                    trait_uses,
                    properties,
                    methods,
                },
                span,
            }]
        }
        StmtKind::ExprStmt(expr) => {
            let expr = prune_expr(expr);
            if expr_has_side_effects(&expr) {
                vec![Stmt {
                    kind: StmtKind::ExprStmt(expr),
                    span,
                }]
            } else {
                Vec::new()
            }
        }
        StmtKind::EnumDecl {
            name,
            backing_type,
            cases,
        } => vec![Stmt {
            kind: StmtKind::EnumDecl {
                name,
                backing_type,
                cases,
            },
            span,
        }],
        StmtKind::PackedClassDecl { name, fields } => vec![Stmt {
            kind: StmtKind::PackedClassDecl { name, fields },
            span,
        }],
        StmtKind::InterfaceDecl {
            name,
            extends,
            methods,
        } => vec![Stmt {
            kind: StmtKind::InterfaceDecl {
                name,
                extends,
                methods: methods
                    .into_iter()
                    .map(dce_method_without_context)
                    .collect(),
            },
            span,
        }],
        StmtKind::TraitDecl {
            name,
            trait_uses,
            properties,
            methods,
        } => vec![Stmt {
            kind: StmtKind::TraitDecl {
                name,
                trait_uses,
                properties,
                methods: methods
                    .into_iter()
                    .map(dce_method_without_context)
                    .collect(),
            },
            span,
        }],
        kind => vec![Stmt { kind, span }],
    }
}

fn guard_variable_name(condition: &Expr) -> Option<(&str, bool)> {
    match &condition.kind {
        ExprKind::Variable(name) => Some((name.as_str(), true)),
        ExprKind::Not(inner) => match &inner.kind {
            ExprKind::Variable(name) => Some((name.as_str(), false)),
            _ => None,
        },
        _ => None,
    }
}

fn scalar_guard_value(expr: &Expr) -> Option<GuardLiteral> {
    match &expr.kind {
        ExprKind::BoolLiteral(value) => Some(GuardLiteral::Bool(*value)),
        ExprKind::Null => Some(GuardLiteral::Null),
        ExprKind::IntLiteral(value) => Some(GuardLiteral::Int(*value)),
        ExprKind::FloatLiteral(value) => Some(GuardLiteral::Float(value.to_bits())),
        ExprKind::StringLiteral(value) => Some(GuardLiteral::String(value.clone())),
        _ => None,
    }
}

fn strict_scalar_guard(condition: &Expr) -> Option<(&str, GuardLiteral, bool)> {
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

fn guard_literal_truthy(value: &GuardLiteral) -> bool {
    match value {
        GuardLiteral::Bool(value) => *value,
        GuardLiteral::Null => false,
        GuardLiteral::Int(value) => *value != 0,
        GuardLiteral::Float(bits) => f64::from_bits(*bits) != 0.0,
        GuardLiteral::String(value) => !value.is_empty() && value != "0",
    }
}

fn known_exact_guard<'a>(guards: &'a GuardState, name: &str) -> Option<&'a GuardLiteral> {
    guards
        .exact_guards
        .iter()
        .find(|known| known.name == name)
        .map(|known| &known.value)
}

fn has_excluded_guard(guards: &GuardState, name: &str, value: &GuardLiteral) -> bool {
    guards
        .excluded_guards
        .iter()
        .any(|known| known.name == name && known.value == *value)
}

fn known_condition_value(condition: &Expr, guards: &GuardState) -> Option<bool> {
    let mut visiting = Vec::new();
    known_condition_value_inner(condition, guards, &mut visiting)
}

fn known_condition_value_inner(
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

fn known_condition_value_base(
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

fn clear_guards_for_name(guards: &mut GuardState, name: &str) {
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

fn extend_guards_for_switch_case(subject: &Expr, patterns: &[Expr], guards: &GuardState) -> GuardState {
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

fn extend_guards_for_switch_case_no_match(
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

fn extend_guards_for_switch_case_no_match_subject(
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

fn extend_guards(guards: &GuardState, condition: &Expr, branch_taken: bool) -> GuardState {
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

fn invalidate_guards_for_stmt(stmt: &Stmt, guards: &mut GuardState) {
    let mut written = Vec::new();
    collect_written_names(stmt, &mut written);
    if written.is_empty() {
        return;
    }

    invalidate_guards_for_written_names(guards, &written);
}

fn invalidated_guards_for_block(guards: &GuardState, stmts: &[Stmt]) -> GuardState {
    let mut written = Vec::new();
    collect_written_names_in_block(stmts, &mut written);
    if written.is_empty() {
        return guards.clone();
    }

    let mut next = guards.clone();
    invalidate_guards_for_written_names(&mut next, &written);
    next
}

fn invalidated_guards_for_throw_paths(guards: &GuardState, stmts: &[Stmt]) -> GuardState {
    if !block_may_throw(stmts) {
        return guards.clone();
    }

    let mut written = Vec::new();
    collect_written_names_on_throw_paths_in_block(stmts, vec![Vec::new()], &mut written);
    if written.is_empty() {
        return guards.clone();
    }

    let mut next = guards.clone();
    invalidate_guards_for_written_names(&mut next, &written);
    next
}

fn invalidated_guards_for_finally_paths(
    guards: &GuardState,
    try_body: &[Stmt],
    catches: &[crate::parser::ast::CatchClause],
) -> GuardState {
    let mut next = invalidated_guards_for_block(guards, try_body);
    for catch in catches {
        next = invalidated_guards_for_block(&next, &catch.body);
        if let Some(variable) = catch.variable.as_deref() {
            clear_guards_for_name(&mut next, variable);
        }
    }
    next
}

fn collect_written_names_on_throw_paths_in_block(
    stmts: &[Stmt],
    mut incoming_paths: Vec<Vec<String>>,
    written: &mut Vec<String>,
) -> Vec<Vec<String>> {
    for stmt in stmts {
        if incoming_paths.is_empty() {
            break;
        }

        let mut next_paths = Vec::new();
        for path in incoming_paths {
            collect_written_names_on_throw_paths_in_stmt(stmt, path, written, &mut next_paths);
        }
        incoming_paths = next_paths;
    }

    incoming_paths
}

fn collect_written_names_on_throw_paths_in_stmt(
    stmt: &Stmt,
    path: Vec<String>,
    written: &mut Vec<String>,
    next_paths: &mut Vec<Vec<String>>,
) {
    match &stmt.kind {
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => {
            if expr_effect(condition).may_throw {
                merge_written_path(written, &path);
            }
            next_paths.extend(collect_written_names_on_throw_paths_in_block(
                then_body,
                vec![path.clone()],
                written,
            ));
            next_paths.extend(collect_written_names_on_throw_paths_in_if_false_path(
                elseif_clauses,
                else_body,
                path,
                written,
            ));
            return;
        }
        StmtKind::IfDef {
            then_body,
            else_body,
            ..
        } => {
            next_paths.extend(collect_written_names_on_throw_paths_in_block(
                then_body,
                vec![path.clone()],
                written,
            ));
            if let Some(body) = else_body {
                next_paths.extend(collect_written_names_on_throw_paths_in_block(
                    body,
                    vec![path],
                    written,
                ));
            } else {
                next_paths.push(path);
            }
            return;
        }
        StmtKind::Switch {
            subject,
            cases,
            default,
        } => {
            if expr_effect(subject).may_throw {
                merge_written_path(written, &path);
            }

            let mut fallthrough_paths = Vec::new();
            for (patterns, body) in cases {
                for pattern in patterns {
                    if expr_effect(pattern).may_throw {
                        merge_written_path(written, &path);
                    }
                }

                let mut incoming = vec![path.clone()];
                incoming.extend(fallthrough_paths);
                fallthrough_paths = collect_written_names_on_throw_paths_in_block(body, incoming, written);
            }

            if let Some(body) = default {
                let mut incoming = vec![path.clone()];
                incoming.extend(fallthrough_paths);
                let _ = collect_written_names_on_throw_paths_in_block(body, incoming, written);
            }

            if matches!(stmt_terminal_effect(stmt), TerminalEffect::FallsThrough) {
                let mut fallthrough = path;
                collect_written_names(stmt, &mut fallthrough);
                next_paths.push(fallthrough);
            }
            return;
        }
        _ => {}
    }

    if stmt_may_throw(stmt) {
        merge_written_path(written, &path);
    }

    if matches!(stmt_terminal_effect(stmt), TerminalEffect::FallsThrough) {
        let mut fallthrough = path;
        collect_written_names(stmt, &mut fallthrough);
        next_paths.push(fallthrough);
    }
}

fn collect_written_names_on_throw_paths_in_if_false_path(
    elseif_clauses: &[(Expr, Vec<Stmt>)],
    else_body: &Option<Vec<Stmt>>,
    path: Vec<String>,
    written: &mut Vec<String>,
) -> Vec<Vec<String>> {
    let Some((condition, body)) = elseif_clauses.first() else {
        return else_body
            .as_ref()
            .map(|body| {
                collect_written_names_on_throw_paths_in_block(body, vec![path.clone()], written)
            })
            .unwrap_or_else(|| vec![path]);
    };

    if expr_effect(condition).may_throw {
        merge_written_path(written, &path);
    }

    let mut next_paths =
        collect_written_names_on_throw_paths_in_block(body, vec![path.clone()], written);
    next_paths.extend(collect_written_names_on_throw_paths_in_if_false_path(
        &elseif_clauses[1..],
        else_body,
        path,
        written,
    ));
    next_paths
}

fn merge_written_path(written: &mut Vec<String>, path: &[String]) {
    for name in path {
        push_written_name(written, name);
    }
}

fn invalidate_guards_for_written_names(guards: &mut GuardState, written: &[String]) {
    guards
        .truthy_vars
        .retain(|name| !written.iter().any(|written_name| written_name == name));
    guards
        .falsy_vars
        .retain(|name| !written.iter().any(|written_name| written_name == name));
    guards
        .bool_true_vars
        .retain(|name| !written.iter().any(|written_name| written_name == name));
    guards
        .bool_false_vars
        .retain(|name| !written.iter().any(|written_name| written_name == name));
    guards
        .exact_guards
        .retain(|known| !written.iter().any(|written_name| written_name == &known.name));
    guards
        .excluded_guards
        .retain(|known| !written.iter().any(|written_name| written_name == &known.name));
    guards
        .condition_guards
        .retain(|known| !known.names.iter().any(|name| written.iter().any(|written_name| written_name == name)));
}

fn collect_written_names(stmt: &Stmt, written: &mut Vec<String>) {
    match &stmt.kind {
        StmtKind::Assign { name, .. }
        | StmtKind::TypedAssign { name, .. }
        | StmtKind::StaticVar { name, .. } => push_written_name(written, name),
        StmtKind::ArrayAssign { array, .. } | StmtKind::ArrayPush { array, .. } => {
            push_written_name(written, array)
        }
        StmtKind::ListUnpack { vars, .. } => {
            for name in vars {
                push_written_name(written, name);
            }
        }
        StmtKind::ExprStmt(expr) => collect_expr_written_names(expr, written),
        StmtKind::If {
            then_body,
            elseif_clauses,
            else_body,
            ..
        } => {
            collect_written_names_in_block(then_body, written);
            for (_, body) in elseif_clauses {
                collect_written_names_in_block(body, written);
            }
            if let Some(body) = else_body {
                collect_written_names_in_block(body, written);
            }
        }
        StmtKind::IfDef {
            then_body,
            else_body,
            ..
        } => {
            collect_written_names_in_block(then_body, written);
            if let Some(body) = else_body {
                collect_written_names_in_block(body, written);
            }
        }
        StmtKind::While { body, .. }
        | StmtKind::DoWhile { body, .. }
        | StmtKind::NamespaceBlock { body, .. } => collect_written_names_in_block(body, written),
        StmtKind::For {
            init,
            update,
            body,
            ..
        } => {
            if let Some(stmt) = init {
                collect_written_names(stmt, written);
            }
            if let Some(stmt) = update {
                collect_written_names(stmt, written);
            }
            collect_written_names_in_block(body, written);
        }
        StmtKind::Foreach {
            key_var,
            value_var,
            body,
            ..
        } => {
            if let Some(name) = key_var {
                push_written_name(written, name);
            }
            push_written_name(written, value_var);
            collect_written_names_in_block(body, written);
        }
        StmtKind::Switch { cases, default, .. } => {
            for (_, body) in cases {
                collect_written_names_in_block(body, written);
            }
            if let Some(body) = default {
                collect_written_names_in_block(body, written);
            }
        }
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            collect_written_names_in_block(try_body, written);
            for catch in catches {
                if let Some(name) = &catch.variable {
                    push_written_name(written, name);
                }
                collect_written_names_in_block(&catch.body, written);
            }
            if let Some(body) = finally_body {
                collect_written_names_in_block(body, written);
            }
        }
        _ => {}
    }
}

fn collect_written_names_in_block(stmts: &[Stmt], written: &mut Vec<String>) {
    for stmt in stmts {
        collect_written_names(stmt, written);
    }
}

fn collect_expr_written_names(expr: &Expr, written: &mut Vec<String>) {
    match &expr.kind {
        ExprKind::PreIncrement(name)
        | ExprKind::PostIncrement(name)
        | ExprKind::PreDecrement(name)
        | ExprKind::PostDecrement(name) => push_written_name(written, name),
        _ => {}
    }
}

fn push_written_name(written: &mut Vec<String>, name: &str) {
    if !written.iter().any(|known| known == name) {
        written.push(name.to_string());
    }
}

pub(crate) fn dce_method(method: ClassMethod, class_name: &str, parent_name: Option<&str>) -> ClassMethod {
    let context = ClassEffectContext {
        class_name: class_name.to_string(),
        parent_name: parent_name.map(str::to_string),
    };
    ClassMethod {
        body: with_class_effect_context(Some(context), || dce_block(method.body)),
        ..method
    }
}

pub(crate) fn dce_method_without_context(method: ClassMethod) -> ClassMethod {
    ClassMethod {
        body: with_class_effect_context(None, || dce_block(method.body)),
        ..method
    }
}
