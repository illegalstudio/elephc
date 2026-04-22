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
    let false_guards = extend_guards(guards, &condition, false);
    let elseif_clauses: Vec<_> = elseif_clauses
        .into_iter()
        .map(|(condition, body)| {
            let condition = prune_expr(condition);
            let body = dce_block_with_guards(body, extend_guards(&false_guards, &condition, true));
            (condition, body)
        })
        .collect();
    let else_body =
        normalize_optional_block(else_body.map(|body| dce_block_with_guards(body, false_guards)));
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

fn dce_switch_stmt(
    subject: Expr,
    cases: Vec<(Vec<Expr>, Vec<Stmt>)>,
    default: Option<Vec<Stmt>>,
    span: crate::span::Span,
    guards: &GuardState,
) -> Vec<Stmt> {
    let trim_switch_noop_break = |body: Vec<Stmt>| {
        if body.len() == 1 && matches!(body[0].kind, StmtKind::Break) {
            Vec::new()
        } else {
            body
        }
    };
    let subject = prune_expr(subject);
    let cases = normalize_switch_cases(drop_shadowed_switch_patterns(normalize_switch_cases(
        cases
            .into_iter()
            .map(|(patterns, body)| {
                let patterns: Vec<_> = patterns.into_iter().map(prune_expr).collect();
                let case_guards = extend_guards_for_switch_case(&subject, &patterns, guards);
                (
                    patterns,
                    trim_switch_noop_break(dce_block_with_guards(body, case_guards)),
                )
            })
            .collect(),
    )));
    let mut cases = cases;
    while cases.last().is_some_and(|(_, body)| body.is_empty()) {
        cases.pop();
    }
    let default =
        normalize_optional_block(default.map(|body| dce_block_with_guards(body, guards.clone())));

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
    let trim_switch_noop_break = |body: Vec<Stmt>| {
        if body.len() == 1 && matches!(body[0].kind, StmtKind::Break) {
            Vec::new()
        } else {
            body
        }
    };
    let subject = prune_expr(subject);
    let tail = dce_block_with_guards(tail, guards.clone());
    let mut cases = normalize_switch_cases(drop_shadowed_switch_patterns(normalize_switch_cases(
        cases
            .into_iter()
            .map(|(patterns, body)| {
                let patterns: Vec<_> = patterns.into_iter().map(prune_expr).collect();
                let case_guards = extend_guards_for_switch_case(&subject, &patterns, guards);
                (
                    patterns,
                    trim_switch_noop_break(dce_block_with_guards(body, case_guards)),
                )
            })
            .collect(),
    )));
    while cases.last().is_some_and(|(_, body)| body.is_empty()) {
        cases.pop();
    }
    let mut default =
        normalize_optional_block(default.map(|body| dce_block_with_guards(body, guards.clone())));

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
    let catches: Vec<_> = catches
        .into_iter()
        .map(|catch| crate::parser::ast::CatchClause {
            exception_types: catch.exception_types,
            variable: catch.variable,
            body: dce_block_with_guards(catch.body, guards.clone()),
        })
        .collect();
    let catches = if block_may_throw(&try_body) {
        normalize_catch_clauses(drop_shadowed_catch_clauses(normalize_catch_clauses(catches)))
    } else {
        Vec::new()
    };
    let finally_body =
        normalize_optional_block(finally_body.map(|body| dce_block_with_guards(body, guards.clone())));

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
    let catches: Vec<_> = catches
        .into_iter()
        .map(|catch| crate::parser::ast::CatchClause {
            exception_types: catch.exception_types,
            variable: catch.variable,
            body: dce_block_with_guards(catch.body, guards.clone()),
        })
        .collect();
    let catches = if block_may_throw(&try_body) {
        normalize_catch_clauses(drop_shadowed_catch_clauses(normalize_catch_clauses(catches)))
    } else {
        Vec::new()
    };
    let finally_body =
        normalize_optional_block(finally_body.map(|body| dce_block_with_guards(body, guards.clone())));
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

fn strict_bool_guard(condition: &Expr) -> Option<(&str, bool, bool)> {
    let ExprKind::BinaryOp { left, op, right } = &condition.kind else {
        return None;
    };

    let (name, value) = match (&left.kind, &right.kind) {
        (ExprKind::Variable(name), ExprKind::BoolLiteral(value))
        | (ExprKind::BoolLiteral(value), ExprKind::Variable(name)) => (name.as_str(), *value),
        _ => return None,
    };

    match op {
        BinOp::StrictEq => Some((name, value, true)),
        BinOp::StrictNotEq => Some((name, value, false)),
        _ => None,
    }
}

fn known_condition_value(condition: &Expr, guards: &GuardState) -> Option<bool> {
    if let Some((name, truthy_if_true)) = guard_variable_name(condition) {
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

    if let Some((name, compared_bool, expects_equal)) = strict_bool_guard(condition) {
        if guards.bool_true_vars.iter().any(|known| known == name) {
            return Some((true == compared_bool) == expects_equal);
        }
        if guards.bool_false_vars.iter().any(|known| known == name) {
            return Some((false == compared_bool) == expects_equal);
        }
    }

    None
}

fn clear_guards_for_name(guards: &mut GuardState, name: &str) {
    guards.truthy_vars.retain(|known| known != name);
    guards.falsy_vars.retain(|known| known != name);
    guards.bool_true_vars.retain(|known| known != name);
    guards.bool_false_vars.retain(|known| known != name);
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

fn record_exact_bool_guard(guards: &mut GuardState, name: &str, value: bool) {
    clear_guards_for_name(guards, name);
    if value {
        push_guard_name(&mut guards.bool_true_vars, name);
    } else {
        push_guard_name(&mut guards.bool_false_vars, name);
    }
    record_truthy_guard(guards, name, value);
}

fn exact_bool_from_guard_branch(condition: &Expr, branch_taken: bool) -> Option<(&str, bool)> {
    let (name, compared_bool, expects_equal) = strict_bool_guard(condition)?;
    match (expects_equal, branch_taken) {
        (true, true) => Some((name, compared_bool)),
        (false, false) => Some((name, compared_bool)),
        _ => None,
    }
}

fn extend_guards_for_switch_case(subject: &Expr, patterns: &[Expr], guards: &GuardState) -> GuardState {
    let ExprKind::BoolLiteral(subject_bool) = subject.kind else {
        return guards.clone();
    };
    let [pattern] = patterns else {
        return guards.clone();
    };

    extend_guards(guards, pattern, subject_bool)
}

fn extend_guards(guards: &GuardState, condition: &Expr, branch_taken: bool) -> GuardState {
    let mut next = guards.clone();

    if let Some((name, exact_bool)) = exact_bool_from_guard_branch(condition, branch_taken) {
        record_exact_bool_guard(&mut next, name, exact_bool);
        return next;
    }

    let Some((name, truthy_if_true)) = guard_variable_name(condition) else {
        return next;
    };

    let known_truthy = if branch_taken { truthy_if_true } else { !truthy_if_true };
    record_truthy_guard(&mut next, name, known_truthy);

    next
}

fn invalidate_guards_for_stmt(stmt: &Stmt, guards: &mut GuardState) {
    let mut written = Vec::new();
    collect_written_names(stmt, &mut written);
    if written.is_empty() {
        return;
    }

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
