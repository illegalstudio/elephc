//! Purpose:
//! Checks programs and statements against the EIR eval AOT subset.
//!
//! Called from:
//! - The eval AOT facade and sibling analysis modules.
//!
//! Key details:
//! - Control-flow merges preserve only facts true on every reachable branch.

use super::*;

/// Returns true when the fragment can be lowered as a no-scope EIR function today.
pub(super) fn program_is_eir_function_safe<S>(program: &[Stmt], support: &S) -> bool
where
    S: EirStaticCallSupport,
{
    let mut facts = EirLocalFacts::new();
    block_is_eir_function_safe(program, support, &mut facts, None, 0).is_some()
}

/// Returns true when a fragment can be lowered as a scope-aware EIR function.
pub(super) fn program_is_eir_scope_function_safe<S>(
    program: &[Stmt],
    support: &S,
    scope_names: &BTreeSet<String>,
) -> bool
where
    S: EirStaticCallSupport,
{
    let mut facts = EirLocalFacts::new();
    block_is_eir_function_safe(program, support, &mut facts, Some(scope_names), 0).is_some()
}

/// Checks a statement block for the no-scope EIR-function eval subset.
pub(super) fn block_is_eir_function_safe<S>(
    body: &[Stmt],
    support: &S,
    facts: &mut EirLocalFacts,
    scope_reads: Option<&BTreeSet<String>>,
    loop_depth: usize,
) -> Option<bool>
where
    S: EirStaticCallSupport,
{
    let mut terminated = false;
    for stmt in body {
        if terminated {
            return None;
        }
        let done = stmt_is_eir_function_safe(stmt, support, facts, scope_reads, loop_depth)?;
        terminated = done;
    }
    Some(terminated)
}

/// Checks one statement for the initial no-scope EIR-function eval subset.
pub(super) fn stmt_is_eir_function_safe<S>(
    stmt: &Stmt,
    support: &S,
    facts: &mut EirLocalFacts,
    scope_reads: Option<&BTreeSet<String>>,
    loop_depth: usize,
) -> Option<bool>
where
    S: EirStaticCallSupport,
{
    match &stmt.kind {
        StmtKind::Synthetic(body) => {
            block_is_eir_function_safe(body, support, facts, scope_reads, loop_depth)
        }
        StmtKind::Echo(expr) => {
            expr_is_eir_function_safe(expr, support, facts, scope_reads).then_some(false)
        }
        StmtKind::ExprStmt(Expr {
            kind:
                ExprKind::Assignment {
                    target,
                    value,
                    result_target,
                    prelude,
                    conditional_value_temp,
                },
            ..
        }) => {
            let ExprKind::Variable(name) = &target.kind else {
                return None;
            };
            if !prelude.is_empty()
                || result_target.is_some()
                || conditional_value_temp.is_some()
                || !scope_reads.is_some_and(|names| names.contains(name))
                || !expr_is_eir_function_safe(value, support, facts, scope_reads)
            {
                return None;
            }
            facts.assign(name, value, support, scope_reads);
            Some(false)
        }
        StmtKind::Assign { name, value }
            if scope_reads.is_some_and(|names| names.contains(name)) =>
        {
            expr_is_eir_function_safe(value, support, facts, scope_reads).then_some(())?;
            facts.assign(name, value, support, scope_reads);
            Some(false)
        }
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => {
            expr_is_eir_function_safe(condition, support, facts, scope_reads).then_some(())?;
            if_stmt_is_eir_function_safe(
                then_body,
                elseif_clauses,
                else_body.as_deref(),
                support,
                facts,
                scope_reads,
                loop_depth,
            )
            .then_some(false)
        }
        StmtKind::While { condition, body } => {
            expr_is_eir_function_safe(condition, support, facts, scope_reads).then_some(())?;
            let mut body_facts = facts.clone();
            block_is_eir_function_safe(body, support, &mut body_facts, scope_reads, loop_depth + 1)
                .map(|_| false)
        }
        StmtKind::DoWhile { condition, body } => {
            let mut body_facts = facts.clone();
            block_is_eir_function_safe(
                body,
                support,
                &mut body_facts,
                scope_reads,
                loop_depth + 1,
            )?;
            expr_is_eir_function_safe(condition, support, &body_facts, scope_reads).then_some(false)
        }
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => {
            if let Some(init) = init {
                if stmt_is_eir_function_safe(init, support, facts, scope_reads, loop_depth)? {
                    return None;
                }
            }
            if let Some(condition) = condition {
                expr_is_eir_function_safe(condition, support, facts, scope_reads).then_some(())?;
            }
            let mut body_facts = facts.clone();
            block_is_eir_function_safe(
                body,
                support,
                &mut body_facts,
                scope_reads,
                loop_depth + 1,
            )?;
            if let Some(update) = update {
                if stmt_is_eir_function_safe(
                    update,
                    support,
                    &mut body_facts,
                    scope_reads,
                    loop_depth + 1,
                )? {
                    return None;
                }
            }
            Some(false)
        }
        StmtKind::Foreach {
            array,
            key_var,
            value_var,
            value_by_ref,
            body,
        } => {
            let static_empty = expr_is_static_empty_array_literal_source(array);
            if (scope_reads.is_none() && !static_empty)
                || *value_by_ref
                || !expr_is_eir_foreach_source_safe(array, scope_reads)
            {
                return None;
            }
            expr_is_eir_foreach_source_lowerable(array, support, facts, scope_reads)
                .then_some(())?;
            let mut body_facts = facts.clone();
            body_facts.assign_unknown(value_var);
            if let Some(key_var) = key_var {
                body_facts.assign_unknown(key_var);
            }
            block_is_eir_function_safe(
                body,
                support,
                &mut body_facts,
                scope_reads,
                loop_depth + 1,
            )?;
            if expr_is_non_empty_static_array_literal_source(array) {
                facts.assign_unknown(value_var);
                if let Some(key_var) = key_var {
                    facts.assign_unknown(key_var);
                }
            }
            Some(false)
        }
        StmtKind::Switch {
            subject,
            cases,
            default,
        } => switch_stmt_is_eir_function_safe(
            subject,
            cases,
            default.as_deref(),
            support,
            facts,
            scope_reads,
            loop_depth,
        )
        .then_some(false),
        StmtKind::Break(level) => (*level > 0 && *level <= loop_depth).then_some(true),
        StmtKind::Continue(level) => (*level > 0 && *level <= loop_depth).then_some(true),
        StmtKind::Return(Some(expr)) => {
            expr_is_eir_function_safe(expr, support, facts, scope_reads).then_some(true)
        }
        StmtKind::Return(None) => Some(true),
        StmtKind::ExprStmt(expr) => match &expr.kind {
            ExprKind::Print(inner) => {
                expr_is_eir_function_safe(inner, support, facts, scope_reads).then_some(false)
            }
            _ => expr_is_eir_function_safe(expr, support, facts, scope_reads).then_some(false),
        },
        _ => None,
    }
}

/// Returns true when a foreach source has EIR-safe eval AOT semantics.
pub(super) fn expr_is_eir_foreach_source_safe(expr: &Expr, scope_reads: Option<&BTreeSet<String>>) -> bool {
    if expr_is_static_array_literal_source(expr) {
        return true;
    }
    matches!(
        &expr.kind,
        ExprKind::Variable(name) if scope_reads.is_some_and(|reads| reads.contains(name))
    )
}

/// Returns true when a foreach source can be lowered by the EIR backend.
pub(super) fn expr_is_eir_foreach_source_lowerable<S>(
    expr: &Expr,
    support: &S,
    facts: &EirLocalFacts,
    scope_reads: Option<&BTreeSet<String>>,
) -> bool
where
    S: EirStaticCallSupport,
{
    match &expr.kind {
        ExprKind::Variable(name) if scope_reads.is_some_and(|reads| reads.contains(name)) => true,
        _ => expr_is_eir_static_array_source_safe(expr, support, facts, scope_reads),
    }
}

/// Returns true when a static array source is a literal expression.
pub(super) fn expr_is_static_array_literal_source(expr: &Expr) -> bool {
    matches!(
        &expr.kind,
        ExprKind::ArrayLiteral(_) | ExprKind::ArrayLiteralAssoc(_)
    )
}

/// Returns true when a static array source is a literal known to skip its body.
pub(super) fn expr_is_static_empty_array_literal_source(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::ArrayLiteral(items) => items.is_empty(),
        ExprKind::ArrayLiteralAssoc(pairs) => pairs.is_empty(),
        _ => false,
    }
}

/// Returns true when a static array source is a literal known to iterate at least once.
pub(super) fn expr_is_non_empty_static_array_literal_source(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::ArrayLiteral(items) => !items.is_empty(),
        ExprKind::ArrayLiteralAssoc(pairs) => !pairs.is_empty(),
        _ => false,
    }
}

/// Checks a switch statement while preserving conservative assignment facts.
pub(super) fn switch_stmt_is_eir_function_safe<S>(
    subject: &Expr,
    cases: &[(Vec<Expr>, Vec<Stmt>)],
    default: Option<&[Stmt]>,
    support: &S,
    facts: &EirLocalFacts,
    scope_reads: Option<&BTreeSet<String>>,
    loop_depth: usize,
) -> bool
where
    S: EirStaticCallSupport,
{
    if !expr_is_eir_function_safe(subject, support, facts, scope_reads) {
        return false;
    }
    if !switch_default_position_is_eir_safe(cases, default) {
        return false;
    }
    for (conditions, body) in cases {
        for condition in conditions {
            if !expr_is_eir_function_safe(condition, support, facts, scope_reads) {
                return false;
            }
        }
        let mut case_facts = facts.clone();
        if block_is_eir_function_safe(body, support, &mut case_facts, scope_reads, loop_depth + 1)
            .is_none()
        {
            return false;
        }
    }
    if let Some(default) = default {
        let mut default_facts = facts.clone();
        if block_is_eir_function_safe(
            default,
            support,
            &mut default_facts,
            scope_reads,
            loop_depth + 1,
        )
        .is_none()
        {
            return false;
        }
    }
    true
}

/// Returns true when EIR switch lowering can reconstruct the default source position.
pub(super) fn switch_default_position_is_eir_safe(
    cases: &[(Vec<Expr>, Vec<Stmt>)],
    default: Option<&[Stmt]>,
) -> bool {
    let Some(default) = default else {
        return true;
    };
    if cases.is_empty() {
        return true;
    }
    let Some(default_start) = default.first().map(|stmt| stmt.span) else {
        return false;
    };
    if default_start == crate::span::Span::dummy() {
        return false;
    }
    cases.iter().all(|(conditions, _)| {
        conditions
            .first()
            .is_some_and(|condition| condition.span != crate::span::Span::dummy())
    })
}

/// Checks an if/elseif/else chain and propagates only definitely assigned locals.
pub(super) fn if_stmt_is_eir_function_safe<S>(
    then_body: &[Stmt],
    elseif_clauses: &[(Expr, Vec<Stmt>)],
    else_body: Option<&[Stmt]>,
    support: &S,
    facts: &mut EirLocalFacts,
    scope_reads: Option<&BTreeSet<String>>,
    loop_depth: usize,
) -> bool
where
    S: EirStaticCallSupport,
{
    let before = facts.clone();
    let mut branch_outputs = Vec::new();
    let mut then_facts = before.clone();
    if block_is_eir_function_safe(then_body, support, &mut then_facts, scope_reads, loop_depth)
        .is_none()
    {
        return false;
    }
    branch_outputs.push(then_facts);

    for (condition, body) in elseif_clauses {
        if !expr_is_eir_function_safe(condition, support, &before, scope_reads) {
            return false;
        }
        let mut branch_facts = before.clone();
        if block_is_eir_function_safe(body, support, &mut branch_facts, scope_reads, loop_depth)
            .is_none()
        {
            return false;
        }
        branch_outputs.push(branch_facts);
    }

    let Some(else_body) = else_body else {
        return true;
    };
    let mut else_facts = before.clone();
    if block_is_eir_function_safe(else_body, support, &mut else_facts, scope_reads, loop_depth)
        .is_none()
    {
        return false;
    }
    branch_outputs.push(else_facts);

    *facts = definitely_assigned_after_eir_branches(before, &branch_outputs);
    true
}

/// Keeps only local facts that are true after every branch in an if/elseif/else chain.
pub(super) fn definitely_assigned_after_eir_branches(
    before: EirLocalFacts,
    branch_outputs: &[EirLocalFacts],
) -> EirLocalFacts {
    let mut definitely = before;
    for name in branch_outputs
        .first()
        .into_iter()
        .flat_map(|branch| branch.assigned.iter())
    {
        if branch_outputs
            .iter()
            .all(|branch| branch.assigned.contains(name))
        {
            definitely.assigned.insert(name.clone());
        }
    }
    for name in branch_outputs
        .first()
        .into_iter()
        .flat_map(|branch| branch.int_locals.iter())
    {
        if branch_outputs
            .iter()
            .all(|branch| branch.int_locals.contains(name))
        {
            definitely.int_locals.insert(name.clone());
        }
    }
    for name in branch_outputs
        .first()
        .into_iter()
        .flat_map(|branch| branch.array_locals.iter())
    {
        if branch_outputs
            .iter()
            .all(|branch| branch.array_locals.contains(name))
        {
            definitely.array_locals.insert(name.clone());
        }
    }
    definitely
}
