//! Purpose:
//! Handles DCE ifs cases.
//! Preserves observable effects while removing unreachable tails, redundant branches, or dead writes.
//!
//! Called from:
//! - `crate::optimize::control::dce`
//!
//! Key details:
//! - The pass must remain conservative around throws, finally blocks, switch fallthrough, method calls, and variable writes.

use super::*;
use super::guards::{extend_guards, known_condition_value};
use super::state::GuardState;

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
pub(super) fn dce_if_stmt(
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
