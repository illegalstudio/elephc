use super::*;
use super::guards::clear_guards_for_name;
use super::state::GuardState;
use super::tail::append_tail_to_fallthrough_path;
use super::writes::{invalidated_guards_for_finally_paths, invalidated_guards_for_throw_paths};

pub(super) fn dce_try_stmt(
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

pub(super) fn dce_try_stmt_with_tail(
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
