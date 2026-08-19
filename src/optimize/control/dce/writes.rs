//! Purpose:
//! Handles DCE writes cases.
//! Preserves observable effects while removing unreachable tails, redundant branches, or dead writes.
//!
//! Called from:
//! - `crate::optimize::control::dce`
//!
//! Key details:
//! - The pass must remain conservative around throws, finally blocks, switch fallthrough, method calls, and variable writes.

use super::*;
use super::guards::{clear_guards_for_name, extend_guards};
use super::state::{GuardState, RelSide};
use crate::optimize::exception_flow::{
    active_expr_thrown_types, active_stmt_thrown_types, active_thrown_types_overlap,
    ThrownTypes,
};

mod finally_paths;

pub(super) use finally_paths::invalidated_guards_for_finally_paths;

/// Invalidates guard state for any variable written by a statement.
/// Collects all written variable names from the statement and removes
/// corresponding entries from the guard state.
pub(super) fn invalidate_guards_for_stmt(stmt: &Stmt, guards: &mut GuardState) {
    apply_guard_invalidation(guards, stmt_invalidation(stmt));
}

/// Advances guard state past one completed statement.
///
/// The statement's complete call-aware write set is invalidated first. An exact
/// `int` typed local declaration then seeds the integer domain for subsequent
/// statements, because reaching them proves the checked assignment completed.
/// A by-ref `foreach` also leaves its iterable root reference-volatile because
/// the value binding survives the loop and may mutate that root later.
pub(super) fn advance_guards_after_stmt(stmt: &Stmt, guards: &mut GuardState) {
    invalidate_guards_for_stmt(stmt, guards);
    match &stmt.kind {
        StmtKind::TypedAssign {
            name,
            type_expr: TypeExpr::Int,
            ..
        } => guards.record_integer_domain(name),
        StmtKind::Foreach {
            array,
            value_by_ref: true,
            ..
        } => mark_foreach_root_reference_volatile(guards, array),
        _ => {}
    }
}

/// Builds conservative body-entry guards for `foreach` from the shared write set.
///
/// By-reference values additionally make the iterable root volatile inside the
/// body, preventing a guard established before a write through the alias from
/// being reused afterward.
pub(super) fn invalidated_guards_for_foreach_body(
    guards: &GuardState,
    array: &Expr,
    key_var: Option<&str>,
    value_var: &str,
    value_by_ref: bool,
    body: &[Stmt],
) -> GuardState {
    let mut next = guards.clone();
    apply_guard_invalidation(
        &mut next,
        foreach_invalidation(array, key_var, value_var, value_by_ref, body),
    );
    if value_by_ref {
        mark_foreach_root_reference_volatile(&mut next, array);
    }
    next
}

/// Clears and permanently disables facts for a by-ref `foreach` iterable root.
fn mark_foreach_root_reference_volatile(guards: &mut GuardState, array: &Expr) {
    if let Some(root) = lvalue_root(array) {
        clear_guards_for_name(guards, root);
        guards.mark_reference_volatile(root);
    }
}

/// Computes guard state after a block of statements, accounting for variables
/// written within the block. Returns a new guard state with guards for
/// written variables removed; returns a clone of the input if no variables
/// are written.
pub(super) fn invalidated_guards_for_block(guards: &GuardState, stmts: &[Stmt]) -> GuardState {
    let mut next = guards.clone();
    apply_guard_invalidation(&mut next, block_invalidation(stmts));
    next
}

/// Computes guard state after explicit writes performed while evaluating an expression.
pub(super) fn invalidated_guards_for_expr(guards: &GuardState, expr: &Expr) -> GuardState {
    let mut next = guards.clone();
    apply_guard_invalidation(&mut next, expr_invalidation(expr));
    next
}

/// Applies a shared targeted invalidation to every GuardState fact domain.
fn apply_guard_invalidation(guards: &mut GuardState, invalidation: Invalidation) {
    match invalidation {
        Invalidation::Names(names) => {
            let names: Vec<String> = names.into_iter().collect();
            invalidate_guards_for_written_names(guards, &names);
        }
        Invalidation::All => {
            let reference_volatile_vars = std::mem::take(&mut guards.reference_volatile_vars);
            *guards = GuardState {
                reference_volatile_vars,
                ..GuardState::default()
            };
        }
    }
}

/// Computes guard state after a block, assuming execution may throw at any point.
/// If the block does not contain throwing statements, returns a clone of `guards`.
/// Otherwise, collects all variables written on throw paths and invalidates
/// corresponding guards in the returned state.
pub(super) fn invalidated_guards_for_throw_paths(
    guards: &GuardState,
    stmts: &[Stmt],
    matching: Option<&ThrownTypes>,
) -> GuardState {
    if !block_may_throw(stmts) {
        return guards.clone();
    }

    let mut written = ThrowPathInvalidation::default();
    collect_written_names_on_throw_paths_in_block(
        stmts,
        vec![Vec::new()],
        &mut written,
        guards,
        matching,
    );
    if written.names.is_empty() && !written.all {
        return guards.clone();
    }

    let mut next = guards.clone();
    if written.all {
        apply_guard_invalidation(&mut next, Invalidation::All);
    } else {
        invalidate_guards_for_written_names(&mut next, &written.names);
    }
    next
}

/// Caller-local invalidation accumulated from exception paths entering one handler.
#[derive(Default)]
struct ThrowPathInvalidation {
    names: Vec<String>,
    all: bool,
}

/// Records prior path writes plus writes the throwing construct itself may perform.
fn record_throw_path_invalidation(
    written: &mut ThrowPathInvalidation,
    path: &[String],
    invalidation: Invalidation,
) {
    merge_written_path(&mut written.names, path);
    match invalidation {
        Invalidation::Names(names) => {
            for name in names {
                push_written_name(&mut written.names, &name);
            }
        }
        Invalidation::All => written.all = true,
    }
}

/// Extends one fallthrough path with a construct's call-aware local writes.
fn extend_throw_path_with_invalidation(
    path: &mut Vec<String>,
    invalidation: Invalidation,
    written: &mut ThrowPathInvalidation,
) {
    match invalidation {
        Invalidation::Names(names) => {
            for name in names {
                push_written_name(path, &name);
            }
        }
        Invalidation::All => written.all = true,
    }
}

/// Recursively collects all variable names that may be written on throw paths
/// through a block of statements, given a set of incoming paths representing
/// variable names already known to be written before entering the block.
/// Returns the set of paths that reach the end of the block (fallthrough paths).
fn collect_written_names_on_throw_paths_in_block(
    stmts: &[Stmt],
    mut incoming_paths: Vec<Vec<String>>,
    written: &mut ThrowPathInvalidation,
    guards: &GuardState,
    matching: Option<&ThrownTypes>,
) -> Vec<Vec<String>> {
    let mut current_guards = guards.clone();
    for stmt in stmts {
        if incoming_paths.is_empty() {
            break;
        }

        let mut next_paths = Vec::new();
        for path in incoming_paths {
            collect_written_names_on_throw_paths_in_stmt(
                stmt,
                path,
                written,
                &mut next_paths,
                &current_guards,
                matching,
            );
        }
        incoming_paths = next_paths;
        invalidate_guards_for_stmt(stmt, &mut current_guards);
    }

    incoming_paths
}

/// Recursively collects variable names written on throw paths through a single
/// statement, updating `written` and `next_paths` accordingly. Handles if/ifdef,
/// switch, try-catch-finally, and generic statements; updates guards for
/// conditional branches and accumulates fallthrough paths.
fn collect_written_names_on_throw_paths_in_stmt(
    stmt: &Stmt,
    path: Vec<String>,
    written: &mut ThrowPathInvalidation,
    next_paths: &mut Vec<Vec<String>>,
    guards: &GuardState,
    matching: Option<&ThrownTypes>,
) {
    match &stmt.kind {
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => {
            let condition_invalidation = expr_invalidation(condition);
            if thrown_types_match(&active_expr_thrown_types(condition), matching) {
                record_throw_path_invalidation(
                    written,
                    &path,
                    condition_invalidation.clone(),
                );
            }
            let mut condition_path = path;
            extend_throw_path_with_invalidation(
                &mut condition_path,
                condition_invalidation,
                written,
            );
            next_paths.extend(collect_written_names_on_throw_paths_in_block(
                then_body,
                vec![condition_path.clone()],
                written,
                &extend_guards(guards, condition, true),
                matching,
            ));
            next_paths.extend(collect_written_names_on_throw_paths_in_if_false_path(
                elseif_clauses,
                else_body,
                condition_path,
                written,
                &extend_guards(guards, condition, false),
                matching,
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
                guards,
                matching,
            ));
            if let Some(body) = else_body {
                next_paths.extend(collect_written_names_on_throw_paths_in_block(
                    body,
                    vec![path],
                    written,
                    guards,
                    matching,
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
            let (direct_case_entries, direct_default_entry) =
                direct_switch_entry_blocks(subject, cases, default.is_some(), guards);
            let cfg = build_switch_cfg(cases, default);
            let mut entry_blocks = direct_case_entries.clone();
            if direct_default_entry {
                if let Some(default_entry) = cfg.default_entry {
                    entry_blocks.push(default_entry);
                }
            }
            let reachable = collect_reachable_cfg_blocks(&cfg.blocks, &entry_blocks);

            let subject_invalidation = expr_invalidation(subject);
            if thrown_types_match(&active_expr_thrown_types(subject), matching) {
                record_throw_path_invalidation(
                    written,
                    &path,
                    subject_invalidation.clone(),
                );
            }
            let mut subject_path = path;
            extend_throw_path_with_invalidation(
                &mut subject_path,
                subject_invalidation,
                written,
            );

            let mut direct_scan_path = subject_path.clone();
            let mut fallthrough_paths = Vec::new();
            for (index, (patterns, body)) in cases.iter().enumerate() {
                let direct_entry = direct_case_entries.contains(&index);
                if !reachable.get(index).copied().unwrap_or_default() {
                    fallthrough_paths.clear();
                    continue;
                }

                if direct_entry {
                    for pattern in patterns {
                        let pattern_invalidation = expr_invalidation(pattern);
                        if thrown_types_match(&active_expr_thrown_types(pattern), matching) {
                            record_throw_path_invalidation(
                                written,
                                &direct_scan_path,
                                pattern_invalidation.clone(),
                            );
                        }
                        extend_throw_path_with_invalidation(
                            &mut direct_scan_path,
                            pattern_invalidation,
                            written,
                        );
                    }
                    fallthrough_paths.push(direct_scan_path.clone());
                }
                let incoming = std::mem::take(&mut fallthrough_paths);
                if incoming.is_empty() {
                    fallthrough_paths = Vec::new();
                } else {
                    fallthrough_paths = collect_written_names_on_throw_paths_in_block(
                        body,
                        incoming,
                        written,
                        guards,
                        matching,
                    );
                }
            }

            if let Some(body) = default {
                let default_entry = cfg.default_entry.unwrap();
                if reachable.get(default_entry).copied().unwrap_or_default() {
                    let mut incoming = Vec::new();
                    if direct_default_entry {
                        incoming.push(subject_path.clone());
                    }
                    incoming.extend(fallthrough_paths);
                    if !incoming.is_empty() {
                        let _ = collect_written_names_on_throw_paths_in_block(
                            body,
                            incoming,
                            written,
                            guards,
                            matching,
                        );
                    }
                }
            }

            if matches!(stmt_terminal_effect(stmt), TerminalEffect::FallsThrough) {
                let mut fallthrough = subject_path;
                extend_throw_path_with_invalidation(
                    &mut fallthrough,
                    stmt_invalidation(stmt),
                    written,
                );
                next_paths.push(fallthrough);
            }
            return;
        }
        StmtKind::Try { .. } => {
            if thrown_types_match(&active_stmt_thrown_types(stmt), matching) {
                record_throw_path_invalidation(written, &path, stmt_invalidation(stmt));
            }
            if matches!(stmt_terminal_effect(stmt), TerminalEffect::FallsThrough) {
                let mut fallthrough = path;
                extend_throw_path_with_invalidation(
                    &mut fallthrough,
                    stmt_invalidation(stmt),
                    written,
                );
                next_paths.push(fallthrough);
            }
            return;
        }
        _ => {}
    }

    if thrown_types_match(&active_stmt_thrown_types(stmt), matching) {
        record_throw_path_invalidation(written, &path, stmt_invalidation(stmt));
    }

    if matches!(stmt_terminal_effect(stmt), TerminalEffect::FallsThrough) {
        let mut fallthrough = path;
        extend_throw_path_with_invalidation(
            &mut fallthrough,
            stmt_invalidation(stmt),
            written,
        );
        next_paths.push(fallthrough);
    }
}

/// Recursively handles the false branch of an if-elseif-else chain,
/// collecting variables written on throw paths within each elseif condition
/// and body, and optionally the else body. Returns paths that reach the end
/// of the processed branches.
fn collect_written_names_on_throw_paths_in_if_false_path(
    elseif_clauses: &[(Expr, Vec<Stmt>)],
    else_body: &Option<Vec<Stmt>>,
    path: Vec<String>,
    written: &mut ThrowPathInvalidation,
    guards: &GuardState,
    matching: Option<&ThrownTypes>,
) -> Vec<Vec<String>> {
    let Some((condition, body)) = elseif_clauses.first() else {
        return else_body
            .as_ref()
            .map(|body| {
                collect_written_names_on_throw_paths_in_block(
                    body,
                    vec![path.clone()],
                    written,
                    guards,
                    matching,
                )
            })
            .unwrap_or_else(|| vec![path]);
    };

    let condition_invalidation = expr_invalidation(condition);
    if thrown_types_match(&active_expr_thrown_types(condition), matching) {
        record_throw_path_invalidation(
            written,
            &path,
            condition_invalidation.clone(),
        );
    }
    let mut condition_path = path;
    extend_throw_path_with_invalidation(
        &mut condition_path,
        condition_invalidation,
        written,
    );

    let mut next_paths = collect_written_names_on_throw_paths_in_block(
        body,
        vec![condition_path.clone()],
        written,
        &extend_guards(guards, condition, true),
        matching,
    );
    next_paths.extend(collect_written_names_on_throw_paths_in_if_false_path(
        &elseif_clauses[1..],
        else_body,
        condition_path,
        written,
        &extend_guards(guards, condition, false),
        matching,
    ));
    next_paths
}

/// Returns whether a throwing source can enter the selected catch route.
fn thrown_types_match(thrown: &ThrownTypes, matching: Option<&ThrownTypes>) -> bool {
    if thrown.is_empty() {
        return false;
    }
    matching.is_none_or(|matching| active_thrown_types_overlap(thrown, matching))
}

/// Appends all variable names from `path` into `written`, deduplicating
/// against existing entries.
fn merge_written_path(written: &mut Vec<String>, path: &[String]) {
    for name in path {
        push_written_name(written, name);
    }
}

/// Removes all guard entries for names present in `written` from `guards`.
/// Clears truthy, falsy, bool-true, bool-false, exact, excluded, and
/// condition guards that reference any written variable.
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
        .integer_domain_vars
        .retain(|name| !written.iter().any(|written_name| written_name == name));
    guards
        .range_guards
        .retain(|known| !written.iter().any(|written_name| written_name == &known.name));
    guards.relational_guards.retain(|known| {
        let mentions_left = match &known.left {
            RelSide::Var(name) => written.iter().any(|written_name| written_name == name),
            RelSide::Int(_) => false,
        };
        let mentions_right = match &known.right {
            RelSide::Var(name) => written.iter().any(|written_name| written_name == name),
            RelSide::Int(_) => false,
        };
        !mentions_left && !mentions_right
    });
    guards
        .condition_guards
        .retain(|known| !known.names.iter().any(|name| written.iter().any(|written_name| written_name == name)));
}

/// Recursively collects all variable names written by a statement and its
/// nested sub-statements, appending them to `written`. Handles assignments,
/// increments/decrements, loop constructs, try-catch, switch, if/ifdef,
/// list unpacking, and expression statements.
fn collect_written_names(stmt: &Stmt, written: &mut Vec<String>) {
    match &stmt.kind {
        StmtKind::Assign { name, .. }
        | StmtKind::TypedAssign { name, .. }
        | StmtKind::StaticVar { name, .. } => push_written_name(written, name),
        StmtKind::RefAssign { target, source } => {
            push_written_name(written, target);
            // Aliasing a plain variable writes through to it; property/call sources
            // do not write a local (their reads are tracked separately).
            if let ExprKind::Variable(source_name) = &source.kind {
                push_written_name(written, source_name);
            }
        }
        StmtKind::ArrayAssign { array, .. } | StmtKind::ArrayPush { array, .. } => {
            push_written_name(written, array)
        }
        StmtKind::ListUnpack { vars, .. } => {
            for name in vars {
                push_written_name(written, name);
            }
        }
        StmtKind::Global { vars } => {
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
        | StmtKind::NamespaceBlock { body, .. }
        | StmtKind::IncludeOnceGuard { body, .. } => {
            collect_written_names_in_block(body, written)
        }
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
            array,
            key_var,
            value_var,
            value_by_ref,
            body,
        } => {
            if *value_by_ref {
                if let Some(root) = lvalue_root(array) {
                    push_written_name(written, root);
                }
            }
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

/// Iterates over a block of statements and collects all written variable
/// names by delegating to `collect_written_names` for each statement.
fn collect_written_names_in_block(stmts: &[Stmt], written: &mut Vec<String>) {
    for stmt in stmts {
        collect_written_names(stmt, written);
    }
}

/// Collects variable names written by expressions: pre/post increment/decrement
/// and assignment expressions. For assignments, also collects names from the
/// target (variable, array access, property access) and recursively from
/// the value and any prelude statements.
fn collect_expr_written_names(expr: &Expr, written: &mut Vec<String>) {
    match &expr.kind {
        ExprKind::PreIncrement(name)
        | ExprKind::PostIncrement(name)
        | ExprKind::PreDecrement(name)
        | ExprKind::PostDecrement(name) => push_written_name(written, name),
        ExprKind::Assignment {
            target,
            value,
            prelude,
            ..
        } => {
            for stmt in prelude {
                collect_written_names(stmt, written);
            }
            collect_expr_written_names(value, written);
            collect_assignment_target_written_names(target, written);
        }
        _ => {}
    }
}

/// Collects variable names written through an assignment target expression.
/// Handles plain variables, array accesses (collecting the array base name),
/// and property accesses. Recurses for complex targets but stops at
/// non-variable non-array-access expressions.
fn collect_assignment_target_written_names(target: &Expr, written: &mut Vec<String>) {
    match &target.kind {
        ExprKind::Variable(name) => push_written_name(written, name),
        ExprKind::ArrayAccess { array, index } => {
            if let ExprKind::Variable(name) = &array.kind {
                push_written_name(written, name);
            }
            collect_expr_written_names(array, written);
            collect_expr_written_names(index, written);
        }
        ExprKind::PropertyAccess { object, .. }
        | ExprKind::NullsafePropertyAccess { object, .. } => {
            collect_expr_written_names(object, written);
        }
        _ => collect_expr_written_names(target, written),
    }
}

/// Appends `name` to `written` if it is not already present (deduplication).
fn push_written_name(written: &mut Vec<String>, name: &str) {
    if !written.iter().any(|known| known == name) {
        written.push(name.to_string());
    }
}
