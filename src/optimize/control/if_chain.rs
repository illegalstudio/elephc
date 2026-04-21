use super::*;

pub(crate) fn prune_if_chain(
    condition: Expr,
    then_body: Vec<Stmt>,
    elseif_clauses: Vec<(Expr, Vec<Stmt>)>,
    else_body: Option<Vec<Stmt>>,
) -> Vec<Stmt> {
    let condition = prune_expr(condition);
    match scalar_value(&condition) {
        Some(value) if value.truthy() => prune_block(then_body),
        Some(_) => prune_else_if_chain(elseif_clauses, else_body),
        None => {
            let span = condition.span;
            let then_body = prune_block(then_body);
            let (kept_elseifs, kept_else) = prune_remaining_elseif_chain(elseif_clauses, else_body);
            let kept_else = normalize_optional_block(kept_else);

            if then_body.is_empty() && kept_elseifs.is_empty() && kept_else.is_none() {
                return expr_to_effect_stmt(condition);
            }

            if then_body.is_empty() {
                let fallback_body =
                    build_if_chain_body(kept_elseifs.clone(), kept_else.clone());
                if !fallback_body.is_empty() {
                    return vec![build_if_stmt(
                        invert_condition(condition),
                        fallback_body,
                        Vec::new(),
                        None,
                        span,
                    )];
                }
            }

            let canonical_else_body =
                normalize_optional_block(Some(build_if_chain_body(kept_elseifs, kept_else)));

            if canonical_else_body.as_ref() == Some(&then_body) {
                let mut stmts = expr_to_effect_stmt(condition);
                stmts.extend(then_body);
                return stmts;
            }

            vec![build_if_stmt(
                condition,
                then_body,
                Vec::new(),
                canonical_else_body,
                span,
            )]
        }
    }
}

pub(crate) fn prune_else_if_chain(
    elseif_clauses: Vec<(Expr, Vec<Stmt>)>,
    else_body: Option<Vec<Stmt>>,
) -> Vec<Stmt> {
    let mut clauses = elseif_clauses.into_iter();
    while let Some((condition, body)) = clauses.next() {
        let condition = prune_expr(condition);
        match scalar_value(&condition) {
            Some(value) if value.truthy() => return prune_block(body),
            Some(_) => continue,
            None => {
                let span = condition.span;
                let remaining: Vec<_> = clauses.collect();
                let (kept_elseifs, kept_else) = prune_remaining_elseif_chain(remaining, else_body);
                let canonical_else_body =
                    normalize_optional_block(Some(build_if_chain_body(kept_elseifs, kept_else)));
                return vec![build_if_stmt(
                    condition,
                    prune_block(body),
                    Vec::new(),
                    canonical_else_body,
                    span,
                )];
            }
        }
    }
    else_body.map(prune_block).unwrap_or_default()
}

pub(crate) fn prune_remaining_elseif_chain(
    elseif_clauses: Vec<(Expr, Vec<Stmt>)>,
    else_body: Option<Vec<Stmt>>,
) -> (Vec<(Expr, Vec<Stmt>)>, Option<Vec<Stmt>>) {
    let mut kept = Vec::new();
    for (condition, body) in elseif_clauses {
        let condition = prune_expr(condition);
        match scalar_value(&condition) {
            Some(value) if value.truthy() => return (kept, Some(prune_block(body))),
            Some(_) => {}
            None => kept.push((condition, prune_block(body))),
        }
    }
    (kept, normalize_optional_block(else_body.map(prune_block)))
}
