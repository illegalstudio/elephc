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
use super::state::GuardState;

pub(super) fn invalidate_guards_for_stmt(stmt: &Stmt, guards: &mut GuardState) {
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

pub(super) fn invalidated_guards_for_throw_paths(guards: &GuardState, stmts: &[Stmt]) -> GuardState {
    if !block_may_throw(stmts) {
        return guards.clone();
    }

    let mut written = Vec::new();
    collect_written_names_on_throw_paths_in_block(stmts, vec![Vec::new()], &mut written, guards);
    if written.is_empty() {
        return guards.clone();
    }

    let mut next = guards.clone();
    invalidate_guards_for_written_names(&mut next, &written);
    next
}

pub(super) fn invalidated_guards_for_finally_paths(
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
    guards: &GuardState,
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
            );
        }
        incoming_paths = next_paths;
        invalidate_guards_for_stmt(stmt, &mut current_guards);
    }

    incoming_paths
}

fn collect_written_names_on_throw_paths_in_stmt(
    stmt: &Stmt,
    path: Vec<String>,
    written: &mut Vec<String>,
    next_paths: &mut Vec<Vec<String>>,
    guards: &GuardState,
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
                &extend_guards(guards, condition, true),
            ));
            next_paths.extend(collect_written_names_on_throw_paths_in_if_false_path(
                elseif_clauses,
                else_body,
                path,
                written,
                &extend_guards(guards, condition, false),
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
            ));
            if let Some(body) = else_body {
                next_paths.extend(collect_written_names_on_throw_paths_in_block(
                    body,
                    vec![path],
                    written,
                    guards,
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

            if expr_effect(subject).may_throw {
                merge_written_path(written, &path);
            }

            let mut fallthrough_paths = Vec::new();
            for (index, (patterns, body)) in cases.iter().enumerate() {
                let direct_entry = direct_case_entries.contains(&index);
                if !reachable.get(index).copied().unwrap_or_default() {
                    fallthrough_paths.clear();
                    continue;
                }

                if direct_entry {
                    for pattern in patterns {
                        if expr_effect(pattern).may_throw {
                            merge_written_path(written, &path);
                        }
                    }
                }

                let mut incoming = Vec::new();
                if direct_entry {
                    incoming.push(path.clone());
                }
                incoming.extend(fallthrough_paths);
                if incoming.is_empty() {
                    fallthrough_paths = Vec::new();
                } else {
                    fallthrough_paths = collect_written_names_on_throw_paths_in_block(
                        body,
                        incoming,
                        written,
                        guards,
                    );
                }
            }

            if let Some(body) = default {
                let default_entry = cfg.default_entry.unwrap();
                if reachable.get(default_entry).copied().unwrap_or_default() {
                    let mut incoming = Vec::new();
                    if direct_default_entry {
                        incoming.push(path.clone());
                    }
                    incoming.extend(fallthrough_paths);
                    if !incoming.is_empty() {
                        let _ = collect_written_names_on_throw_paths_in_block(
                            body,
                            incoming,
                            written,
                            guards,
                        );
                    }
                }
            }

            if matches!(stmt_terminal_effect(stmt), TerminalEffect::FallsThrough) {
                let mut fallthrough = path;
                collect_written_names(stmt, &mut fallthrough);
                next_paths.push(fallthrough);
            }
            return;
        }
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            next_paths.extend(collect_written_names_on_throw_paths_in_block(
                try_body,
                vec![path.clone()],
                written,
                guards,
            ));
            for catch in catches {
                let mut catch_path = path.clone();
                if let Some(variable) = catch.variable.as_deref() {
                    push_written_name(&mut catch_path, variable);
                }
                next_paths.extend(collect_written_names_on_throw_paths_in_block(
                    &catch.body,
                    vec![catch_path],
                    written,
                    guards,
                ));
            }
            if let Some(body) = finally_body {
                next_paths.extend(collect_written_names_on_throw_paths_in_block(
                    body,
                    vec![path],
                    written,
                    guards,
                ));
            } else if matches!(stmt_terminal_effect(stmt), TerminalEffect::FallsThrough) {
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
    guards: &GuardState,
) -> Vec<Vec<String>> {
    let Some((condition, body)) = elseif_clauses.first() else {
        return else_body
            .as_ref()
            .map(|body| {
                collect_written_names_on_throw_paths_in_block(body, vec![path.clone()], written, guards)
            })
            .unwrap_or_else(|| vec![path]);
    };

    if expr_effect(condition).may_throw {
        merge_written_path(written, &path);
    }

    let mut next_paths = collect_written_names_on_throw_paths_in_block(
        body,
        vec![path.clone()],
        written,
        &extend_guards(guards, condition, true),
    );
    next_paths.extend(collect_written_names_on_throw_paths_in_if_false_path(
        &elseif_clauses[1..],
        else_body,
        path,
        written,
        &extend_guards(guards, condition, false),
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

fn push_written_name(written: &mut Vec<String>, name: &str) {
    if !written.iter().any(|known| known == name) {
        written.push(name.to_string());
    }
}
