use super::*;

pub(crate) fn expr_to_effect_stmt(expr: Expr) -> Vec<Stmt> {
    let span = expr.span;
    if expr_is_observable(&expr) {
        vec![Stmt::new(StmtKind::ExprStmt(expr), span)]
    } else {
        Vec::new()
    }
}

pub(crate) fn normalize_optional_block(body: Option<Vec<Stmt>>) -> Option<Vec<Stmt>> {
    body.filter(|body| !body.is_empty())
}

pub(crate) fn normalize_exception_types(exception_types: Vec<Name>) -> Vec<Name> {
    let mut normalized = Vec::new();
    for exception_type in exception_types {
        if !normalized.contains(&exception_type) {
            normalized.push(exception_type);
        }
    }
    normalized.sort_by(|left, right| left.as_str().cmp(right.as_str()));
    normalized
}

pub(crate) fn normalize_catch_clauses(
    catches: Vec<crate::parser::ast::CatchClause>,
) -> Vec<crate::parser::ast::CatchClause> {
    let mut normalized: Vec<crate::parser::ast::CatchClause> = Vec::new();
    for mut catch in catches {
        catch.exception_types = normalize_exception_types(catch.exception_types);
        if let Some(last) = normalized.last_mut() {
            if last.variable == catch.variable && last.body == catch.body {
                last.exception_types.extend(catch.exception_types);
                last.exception_types = normalize_exception_types(std::mem::take(&mut last.exception_types));
                continue;
            }
        }
        normalized.push(catch);
    }
    normalized
}

pub(crate) fn normalize_switch_cases(cases: Vec<(Vec<Expr>, Vec<Stmt>)>) -> Vec<(Vec<Expr>, Vec<Stmt>)> {
    let mut normalized: Vec<(Vec<Expr>, Vec<Stmt>)> = Vec::new();
    let mut pending_fallthrough_patterns: Vec<Expr> = Vec::new();
    for (mut patterns, body) in cases {
        if body.is_empty() {
            pending_fallthrough_patterns.extend(patterns);
            continue;
        }

        if !pending_fallthrough_patterns.is_empty() {
            pending_fallthrough_patterns.append(&mut patterns);
            patterns = pending_fallthrough_patterns;
            pending_fallthrough_patterns = Vec::new();
        }

        if !body.is_empty() {
            if let Some((last_patterns, last_body)) = normalized.last_mut() {
                if *last_body == body {
                    last_patterns.extend(patterns);
                    continue;
                }
            }
        }
        normalized.push((patterns, body));
    }

    if !pending_fallthrough_patterns.is_empty() {
        normalized.push((pending_fallthrough_patterns, Vec::new()));
    }

    normalized
}

pub(crate) fn invert_condition(condition: Expr) -> Expr {
    let span = condition.span;
    prune_expr(Expr::new(ExprKind::Not(Box::new(condition)), span))
}

pub(crate) fn build_if_chain_body(
    elseif_clauses: Vec<(Expr, Vec<Stmt>)>,
    else_body: Option<Vec<Stmt>>,
) -> Vec<Stmt> {
    if let Some(((condition, then_body), rest)) = elseif_clauses.split_first() {
        let nested_else_body = normalize_optional_block(Some(build_if_chain_body(
            rest.to_vec(),
            else_body,
        )));
        vec![Stmt::new(
            StmtKind::If {
                condition: condition.clone(),
                then_body: then_body.clone(),
                elseif_clauses: Vec::new(),
                else_body: nested_else_body,
            },
            condition.span,
        )]
    } else {
        else_body.unwrap_or_default()
    }
}

pub(crate) fn materialize_switch_execution(
    cases: &[(Vec<Expr>, Vec<Stmt>)],
    default: &Option<Vec<Stmt>>,
    start_case_index: Option<usize>,
) -> Vec<Stmt> {
    let mut out = Vec::new();

    let push_body = |body: &[Stmt], out: &mut Vec<Stmt>| -> bool {
        for stmt in body.iter().cloned() {
            if matches!(stmt.kind, StmtKind::Break) {
                return true;
            }

            let stops_here = !matches!(stmt_terminal_effect(&stmt), TerminalEffect::FallsThrough);
            out.push(stmt);
            if stops_here {
                return true;
            }
        }

        false
    };

    if let Some(start_case_index) = start_case_index {
        for (_, body) in &cases[start_case_index..] {
            if push_body(body, &mut out) {
                return out;
            }
        }
    }

    if let Some(default_body) = default {
        let _ = push_body(default_body, &mut out);
    }

    out
}

pub(crate) fn split_hoistable_try_prefix(mut try_body: Vec<Stmt>) -> (Vec<Stmt>, Vec<Stmt>) {
    let hoist_len = try_body
        .iter()
        .take_while(|stmt| {
            !stmt_may_throw(stmt)
                && matches!(stmt_terminal_effect(stmt), TerminalEffect::FallsThrough)
        })
        .count();
    let tail = try_body.split_off(hoist_len);
    (try_body, tail)
}

pub(crate) fn combine_if_conditions(left: Expr, right: Expr) -> Expr {
    let span = left.span;
    prune_expr(Expr::new(
        ExprKind::BinaryOp {
            left: Box::new(left),
            op: BinOp::And,
            right: Box::new(right),
        },
        span,
    ))
}

pub(crate) fn combine_if_chain_conditions(left: Expr, right: Expr) -> Expr {
    let span = left.span;
    prune_expr(Expr::new(
        ExprKind::BinaryOp {
            left: Box::new(left),
            op: BinOp::Or,
            right: Box::new(right),
        },
        span,
    ))
}

pub(crate) fn build_switch_match_condition(subject: &Expr, patterns: &[Expr]) -> Option<Expr> {
    if patterns.is_empty() {
        return None;
    }

    if patterns.len() > 1 && expr_is_observable(subject) {
        return None;
    }

    let mut comparisons = patterns.iter().cloned().map(|pattern| {
        Expr::new(
            ExprKind::BinaryOp {
                left: Box::new(subject.clone()),
                op: BinOp::Eq,
                right: Box::new(pattern),
            },
            subject.span,
        )
    });
    let mut condition = comparisons.next()?;
    for comparison in comparisons {
        condition = Expr::new(
            ExprKind::BinaryOp {
                left: Box::new(condition),
                op: BinOp::Or,
                right: Box::new(comparison),
            },
            subject.span,
        );
    }
    Some(prune_expr(condition))
}
