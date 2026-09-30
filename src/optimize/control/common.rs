//! Purpose:
//! Implements optimizer control-flow common logic.
//! Supports normalization, reachability, path analysis, and structural rewrites used by pruning and DCE.
//!
//! Called from:
//! - `crate::optimize::control`
//!
//! Key details:
//! - Control-flow helpers must treat terminal effects, switch fallthrough, and exception paths conservatively.

use super::*;

/// Converts an expression to an effect-only statement if it is observable.
/// Returns a vector containing the expression wrapped in an `ExprStmt` if
/// `expr_is_observable` returns true; otherwise returns an empty vector.
/// Preserves the expression's span for accurate error reporting.
pub(crate) fn expr_to_effect_stmt(expr: Expr) -> Vec<Stmt> {
    let span = expr.span;
    if expr_is_observable(&expr) {
        vec![Stmt::new(StmtKind::ExprStmt(expr), span)]
    } else {
        Vec::new()
    }
}

/// Normalizes an optional block by removing `None` and empty bodies.
/// Filters out `None` and empty vectors, returning `None` only when the body
/// is `None` or truly empty. Used to prune no-op control-flow branches.
pub(crate) fn normalize_optional_block(body: Option<Vec<Stmt>>) -> Option<Vec<Stmt>> {
    body.filter(|body| !body.is_empty())
}

/// Normalizes a list of exception type names by deduplicating and sorting.
/// Removes duplicate exception types and sorts the remaining ones lexicographically
/// by their string representation. Used to canonicalize catch clause exception lists.
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

/// Normalizes catch clauses by deduplicating exception types and merging
/// catch blocks with identical variable names and bodies.
/// Merges exception type lists for catches sharing the same variable and body,
/// then re-normalizes the merged list. Preserves catch order for semantic correctness.
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

/// Drops catch clauses that are shadowed by previously seen exception types or
/// by a catch targeting `Throwable`. A catch is shadowed when all its exception
/// types have already been caught by an earlier clause. Stops processing at the
/// first `Throwable` catch since it absorbs all throwables.
pub(crate) fn drop_shadowed_catch_clauses(
    catches: Vec<crate::parser::ast::CatchClause>,
) -> Vec<crate::parser::ast::CatchClause> {
    let mut normalized = Vec::new();
    let mut seen_types: Vec<String> = Vec::new();
    let mut catches_all_throwables = false;

    for mut catch in catches {
        if catches_all_throwables {
            break;
        }

        catch.exception_types.retain(|exception_type| {
            !seen_types
                .iter()
                .any(|seen| seen == exception_type.as_str())
        });

        if catch.exception_types.is_empty() {
            continue;
        }

        if catch
            .exception_types
            .iter()
            .any(|exception_type| exception_type.as_str() == "Throwable")
        {
            catches_all_throwables = true;
        }

        for exception_type in &catch.exception_types {
            let exception_type = exception_type.as_str().to_string();
            if !seen_types.contains(&exception_type) {
                seen_types.push(exception_type);
            }
        }

        normalized.push(catch);
    }

    normalized
}

/// Normalizes switch cases by merging empty-body cases into subsequent cases
/// as fallthrough patterns and combining adjacent cases with identical bodies.
/// Empty-body cases accumulate their patterns into a pending fallthrough list
/// that gets merged into the next non-empty case. Adjacent cases with the same
/// body are merged to eliminate redundant case labels.
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

/// Drops switch patterns that have already been seen, merging their body into
/// the previous case if that case falls through. If a pattern is a duplicate of
/// an earlier pattern, it is removed and its body is appended to the previous
/// case's body (only if that case's terminal effect is `FallsThrough`).
pub(crate) fn drop_shadowed_switch_patterns(
    cases: Vec<(Vec<Expr>, Vec<Stmt>)>,
) -> Vec<(Vec<Expr>, Vec<Stmt>)> {
    let mut normalized: Vec<(Vec<Expr>, Vec<Stmt>)> = Vec::new();
    let mut seen_patterns: Vec<Expr> = Vec::new();

    for (mut patterns, body) in cases {
        patterns.retain(|pattern| {
            if seen_patterns.iter().any(|seen| seen == pattern) {
                false
            } else {
                seen_patterns.push(pattern.clone());
                true
            }
        });

        if patterns.is_empty() {
            if let Some((_, previous_body)) = normalized.last_mut() {
                if matches!(block_terminal_effect(previous_body), TerminalEffect::FallsThrough) {
                    previous_body.extend(body);
                }
            }
            continue;
        }

        normalized.push((patterns, body));
    }

    normalized
}

/// Inverts a condition expression by wrapping it in a logical NOT and then
/// applying expression pruning. The returned expression has the same span as
/// the input condition. Used when converting if-else logic during normalization.
pub(crate) fn invert_condition(condition: Expr) -> Expr {
    let span = condition.span;
    prune_expr(Expr::new(ExprKind::Not(Box::new(condition)), span))
}

/// Recursively builds a nested if-else chain from a flat list of elseif clauses
/// and a terminal else body. Each recursion level consumes the first elseif clause
/// and nests the remainder under its else branch. Returns the else_body when no
/// elseif clauses remain. Used to restructure flattened if-else chains.
pub(crate) fn build_if_chain_body(
    elseif_clauses: Vec<(Expr, Vec<Stmt>)>,
    else_body: Option<Vec<Stmt>>,
) -> Vec<Stmt> {
    let mut clauses = elseif_clauses.into_iter();
    let Some((condition, then_body)) = clauses.next() else {
        return else_body.unwrap_or_default();
    };
    let nested_else_body =
        normalize_optional_block(Some(build_if_chain_body(clauses.collect(), else_body)));
    let span = condition.span;
    vec![Stmt::new(
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses: Vec::new(),
            else_body: nested_else_body,
        },
        span,
    )]
}

/// Materializes the effective execution path of a switch starting from an
/// optional case index. Collects statements from each case body until a `Break`
/// is encountered or a statement with a non-fallthrough terminal effect is found.
/// If `start_case_index` is `None`, starts from the default body only.
/// Used to determine what code is actually reachable in a switch.
pub(crate) fn materialize_switch_execution(
    cases: &[(Vec<Expr>, Vec<Stmt>)],
    default: &Option<Vec<Stmt>>,
    start_case_index: Option<usize>,
) -> Vec<Stmt> {
    let mut out = Vec::new();

    let push_body = |body: &[Stmt], out: &mut Vec<Stmt>| -> bool {
        for stmt in body.iter().cloned() {
            if matches!(stmt.kind, StmtKind::Break(1)) {
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

/// Returns `true` if any case body or the default body contains a level-sensitive
/// loop exit (a `Break` with depth > 1, or any `Continue`). Such exits cannot be
/// duplicated or reordered without changing control-flow semantics.
pub(crate) fn switch_has_level_sensitive_loop_exit(
    cases: &[(Vec<Expr>, Vec<Stmt>)],
    default: &Option<Vec<Stmt>>,
) -> bool {
    cases
        .iter()
        .any(|(_, body)| block_has_level_sensitive_loop_exit(body))
        || default
            .as_ref()
            .is_some_and(|body| block_has_level_sensitive_loop_exit(body))
}

/// Returns `true` if the statement list contains a level-sensitive loop exit.
fn block_has_level_sensitive_loop_exit(body: &[Stmt]) -> bool {
    body.iter().any(stmt_has_level_sensitive_loop_exit)
}

/// Returns `true` if the statement contains a level-sensitive loop exit:
/// `Break(n)` where n > 1, or any `Continue`. Recursively checks nested
/// structures including synthetic statements, if/ifdef, loops, switch, and try-catch.
fn stmt_has_level_sensitive_loop_exit(stmt: &Stmt) -> bool {
    match &stmt.kind {
        StmtKind::Break(levels) => *levels > 1,
        StmtKind::Continue(_) => true,
        StmtKind::Synthetic(stmts) => block_has_level_sensitive_loop_exit(stmts),
        StmtKind::If {
            then_body,
            elseif_clauses,
            else_body,
            ..
        } => {
            block_has_level_sensitive_loop_exit(then_body)
                || elseif_clauses
                    .iter()
                    .any(|(_, body)| block_has_level_sensitive_loop_exit(body))
                || else_body
                    .as_ref()
                    .is_some_and(|body| block_has_level_sensitive_loop_exit(body))
        }
        StmtKind::IfDef {
            then_body, else_body, ..
        } => {
            block_has_level_sensitive_loop_exit(then_body)
                || else_body
                    .as_ref()
                    .is_some_and(|body| block_has_level_sensitive_loop_exit(body))
        }
        StmtKind::While { body, .. }
        | StmtKind::DoWhile { body, .. }
        | StmtKind::For { body, .. }
        | StmtKind::Foreach { body, .. }
        | StmtKind::IncludeOnceGuard { body, .. } => block_has_level_sensitive_loop_exit(body),
        StmtKind::Switch { cases, default, .. } => {
            switch_has_level_sensitive_loop_exit(cases, default)
        }
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            block_has_level_sensitive_loop_exit(try_body)
                || catches
                    .iter()
                    .any(|catch| block_has_level_sensitive_loop_exit(&catch.body))
                || finally_body
                    .as_ref()
                    .is_some_and(|body| block_has_level_sensitive_loop_exit(body))
        }
        _ => false,
    }
}

/// Returns `true` when a case or default body contains a STRAY `break` that targets the SWITCH.
///
/// The rewrites below drop the switch, so such a break would lose its target: measured,
/// `case 1: if ($x > 0) { break; } return 1;` compiled to `unreachable` and the program died with
/// an illegal instruction where php answers a value.
///
/// ⚠️ "Stray" is the whole rule, and the first version of this guard did not have it. A `break`
/// ENDING a case body is that body's ordinary terminator, which every rewrite consumes — refusing
/// on it refuses nearly every switch ever written, and it turned off constant switch folding
/// wholesale (six optimizer tests, from `switch (3) { case 1: … case 3: echo 20; break; }` folding
/// to `echo 20;` down to no fold at all). `strip_final_switch_break` removes only the break of the
/// body that runs LAST; every other case keeps its own, and those are not the hazard.
///
/// What IS the hazard is a break the fold cannot consume: one with statements after it, or one
/// nested inside an `if`, a `Synthetic` block or a `try`, where it is conditional.
pub(crate) fn switch_has_body_targeting_break(
    cases: &[(Vec<Expr>, Vec<Stmt>)],
    default: &Option<Vec<Stmt>>,
) -> bool {
    cases
        .iter()
        .any(|(_, body)| case_body_has_stray_switch_break(body))
        || default
            .as_ref()
            .is_some_and(|body| case_body_has_stray_switch_break(body))
}

/// Returns `true` if a case or default body holds a switch-targeting `break` a rewrite cannot
/// consume — anything but a plain `break` as the body's LAST statement.
fn case_body_has_stray_switch_break(body: &[Stmt]) -> bool {
    let last = body.len().saturating_sub(1);
    body.iter().enumerate().any(|(index, stmt)| {
        if matches!(stmt.kind, StmtKind::Break(levels) if levels <= 1) {
            // The terminator every rewrite already drops — unless something follows it, in which
            // case it is not the terminator at all.
            return index != last;
        }
        stmt_has_body_targeting_break(stmt)
    })
}

/// Returns `true` if the statement list contains a switch-targeting `break`, anywhere.
///
/// Used for NESTED blocks, where nothing is exempt: a break at the end of an `if` arm is still
/// conditional, and the rewrite has no way to consume it.
fn block_has_body_targeting_break(body: &[Stmt]) -> bool {
    body.iter().any(stmt_has_body_targeting_break)
}

/// Returns `true` if the statement contains a `break` that would target the enclosing switch.
///
/// ⚠️ The walk deliberately does NOT descend into a loop or a nested switch: a level-1 `break`
/// inside one of those targets IT, not the switch being rewritten, and counting it would refuse a
/// rewrite that is perfectly safe. `Break(n > 1)` is left to
/// [`stmt_has_level_sensitive_loop_exit`], which already refuses it.
fn stmt_has_body_targeting_break(stmt: &Stmt) -> bool {
    match &stmt.kind {
        StmtKind::Break(levels) => *levels <= 1,
        StmtKind::Synthetic(stmts) => block_has_body_targeting_break(stmts),
        StmtKind::If {
            then_body,
            elseif_clauses,
            else_body,
            ..
        } => {
            block_has_body_targeting_break(then_body)
                || elseif_clauses
                    .iter()
                    .any(|(_, body)| block_has_body_targeting_break(body))
                || else_body
                    .as_ref()
                    .is_some_and(|body| block_has_body_targeting_break(body))
        }
        StmtKind::IfDef {
            then_body, else_body, ..
        } => {
            block_has_body_targeting_break(then_body)
                || else_body
                    .as_ref()
                    .is_some_and(|body| block_has_body_targeting_break(body))
        }
        // A try does not capture a break; its body's break still leaves the switch.
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            block_has_body_targeting_break(try_body)
                || catches
                    .iter()
                    .any(|catch| block_has_body_targeting_break(&catch.body))
                || finally_body
                    .as_ref()
                    .is_some_and(|body| block_has_body_targeting_break(body))
        }
        _ => false,
    }
}

/// Splits a try body into a hoistable prefix and a non-hoistable tail.
/// The hoistable prefix contains only statements that may not throw and
/// always fall through. The tail contains the first statement that may throw
/// or any statement with a non-fallthrough terminal effect.
/// When the `try` has a `finally` (`protects_finally`), a statement that can
/// leave early through a nested `return` / `break` / `continue` also ends the
/// prefix: hoisted out of the `try`, that exit would skip the `finally`.
/// Used to separate invariant code from throwing code in try blocks.
pub(crate) fn split_hoistable_try_prefix(
    mut try_body: Vec<Stmt>,
    protects_finally: bool,
) -> (Vec<Stmt>, Vec<Stmt>) {
    let hoist_len = try_body
        .iter()
        .take_while(|stmt| {
            !stmt_may_throw(stmt)
                && matches!(stmt_terminal_effect(stmt), TerminalEffect::FallsThrough)
                && !(protects_finally && block_has_early_exit(std::slice::from_ref(stmt), 0))
        })
        .count();
    let tail = try_body.split_off(hoist_len);
    (try_body, tail)
}

/// Combines two expressions into a logical AND expression, then prunes it.
/// Used when merging consecutive if conditions that must both be true.
/// The result preserves the left expression's span for source location accuracy.
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

/// Combines two expressions into a logical OR expression, then prunes it.
/// Used when merging consecutive elseif conditions where either may be true.
/// The result preserves the left expression's span for source location accuracy.
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

/// Builds a match condition for switch case patterns against a subject.
/// Returns `None` if patterns are empty or if the subject is observable and
/// there are multiple patterns (to avoid evaluating the subject multiple times).
/// Otherwise returns a chain of equality comparisons joined by OR.
/// Each pattern comparison is subject == pattern, and the chain is then pruned.
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

/// Returns whether `body` can leave its enclosing block on a path other than falling off its
/// end: a `return`, a `throw` statement, or a `break` / `continue` that reaches past the
/// `depth` loop / `switch` shells between `body` and that block (a direct body asks with `0`).
/// A `try { body } finally { F }` may only be flattened into `body; F` when this is `false`,
/// because every such exit would otherwise skip `F`. `exit` / `die` are not exits here: PHP
/// terminates without running `finally`, so flattening keeps their behavior.
pub(crate) fn block_has_early_exit(body: &[Stmt], depth: usize) -> bool {
    body.iter().any(|stmt| stmt_has_early_exit(stmt, depth))
}

/// Statement-level walker for `block_has_early_exit`.
fn stmt_has_early_exit(stmt: &Stmt, depth: usize) -> bool {
    match &stmt.kind {
        StmtKind::Return(_) | StmtKind::Throw(_) => true,
        StmtKind::Break(levels) | StmtKind::Continue(levels) => *levels > depth,
        StmtKind::Synthetic(stmts)
        | StmtKind::IncludeOnceGuard { body: stmts, .. }
        | StmtKind::NamespaceBlock { body: stmts, .. } => block_has_early_exit(stmts, depth),
        StmtKind::If {
            then_body,
            elseif_clauses,
            else_body,
            ..
        } => {
            block_has_early_exit(then_body, depth)
                || elseif_clauses
                    .iter()
                    .any(|(_, body)| block_has_early_exit(body, depth))
                || else_body
                    .as_ref()
                    .is_some_and(|body| block_has_early_exit(body, depth))
        }
        StmtKind::IfDef {
            then_body, else_body, ..
        } => {
            block_has_early_exit(then_body, depth)
                || else_body
                    .as_ref()
                    .is_some_and(|body| block_has_early_exit(body, depth))
        }
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            block_has_early_exit(try_body, depth)
                || catches
                    .iter()
                    .any(|catch| block_has_early_exit(&catch.body, depth))
                || finally_body
                    .as_ref()
                    .is_some_and(|body| block_has_early_exit(body, depth))
        }
        StmtKind::While { body, .. }
        | StmtKind::DoWhile { body, .. }
        | StmtKind::For { body, .. }
        | StmtKind::Foreach { body, .. } => block_has_early_exit(body, depth + 1),
        StmtKind::Switch { cases, default, .. } => {
            cases
                .iter()
                .any(|(_, body)| block_has_early_exit(body, depth + 1))
                || default
                    .as_ref()
                    .is_some_and(|body| block_has_early_exit(body, depth + 1))
        }
        _ => false,
    }
}

/// Returns whether the `default` body is the last body of the switch in source order, which is
/// the position EIR lowering gives it: the AST keeps `default` apart from the cases, and
/// `ir_lower::stmt::switches::switch_default_source_index` recovers its place from spans, so
/// this mirrors that rule exactly. A default with no statements, a dummy span on the default or
/// on any case pattern, or an empty case list all lower with the default last.
pub(crate) fn switch_default_runs_last(cases: &[(Vec<Expr>, Vec<Stmt>)], default: &[Stmt]) -> bool {
    let Some(default_start) = default.first().map(|stmt| stmt.span) else {
        return true;
    };
    if default_start == crate::span::Span::dummy() {
        return true;
    }
    for (patterns, _) in cases {
        let Some(case_start) = patterns.first().map(|pattern| pattern.span) else {
            return true;
        };
        if case_start == crate::span::Span::dummy() {
            return true;
        }
        let case_is_after = case_start.line > default_start.line
            || (case_start.line == default_start.line && case_start.col >= default_start.col);
        if case_is_after {
            return false;
        }
    }
    true
}
