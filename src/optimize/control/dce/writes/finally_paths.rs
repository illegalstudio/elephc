//! Purpose:
//! Computes guard invalidation from writes on paths that actually enter an enclosing `finally`.
//! Distinguishes PHP control transfers from process-exit paths, which skip `finally` execution.
//!
//! Called from:
//! - `crate::optimize::control::dce::tries`
//!
//! Key details:
//! - Branch conditions contribute writes and throw transfers before either branch body runs.
//! - `exit` and `die` paths are excluded because PHP terminates without executing `finally`.

use super::*;

/// Computes guard state through a try-catch-finally construct.
///
/// The result invalidates only variables written on paths that can enter the
/// finally body. Catch variables are included only for reachable catch paths.
pub(in crate::optimize::control::dce) fn invalidated_guards_for_finally_paths(
    guards: &GuardState,
    try_body: &[Stmt],
    catches: &[crate::parser::ast::CatchClause],
) -> GuardState {
    let mut written = writes_on_paths_entering_finally(try_body);
    for catch in catches {
        let catch_writes = writes_on_paths_entering_finally(&catch.body);
        if !catch_writes.is_empty() || block_has_path_entering_finally(&catch.body) {
            merge_written_path(&mut written, &catch_writes);
            if let Some(variable) = catch.variable.as_deref() {
                push_written_name(&mut written, variable);
            }
        }
    }
    let mut next = guards.clone();
    invalidate_guards_for_written_names(&mut next, &written);
    next
}

/// Path sets produced while determining which writes can precede an enclosing finally body.
#[derive(Default)]
struct FinallyWritePaths {
    continuing: Vec<Vec<String>>,
    transfers: Vec<Vec<String>>,
}

/// Returns the union of names written on normal or control-transfer paths entering finally.
fn writes_on_paths_entering_finally(stmts: &[Stmt]) -> Vec<String> {
    let paths = collect_finally_write_paths_in_block(stmts, vec![Vec::new()]);
    let mut written = Vec::new();
    for path in paths.continuing.iter().chain(paths.transfers.iter()) {
        merge_written_path(&mut written, path);
    }
    written
}

/// Returns whether a block has any fallthrough/return/throw/break path that executes finally.
fn block_has_path_entering_finally(stmts: &[Stmt]) -> bool {
    let paths = collect_finally_write_paths_in_block(stmts, vec![Vec::new()]);
    !paths.continuing.is_empty() || !paths.transfers.is_empty()
}

/// Advances write paths through a block while separating fallthrough from finally-triggering transfers.
fn collect_finally_write_paths_in_block(
    stmts: &[Stmt],
    mut incoming: Vec<Vec<String>>,
) -> FinallyWritePaths {
    let mut transfers = Vec::new();
    for stmt in stmts {
        if incoming.is_empty() {
            break;
        }
        let mut next = Vec::new();
        for path in incoming {
            let flow = collect_finally_write_paths_in_stmt(stmt, path);
            next.extend(flow.continuing);
            transfers.extend(flow.transfers);
        }
        incoming = next;
    }
    FinallyWritePaths {
        continuing: incoming,
        transfers,
    }
}

/// Classifies one statement's write paths, excluding process-exit paths that skip PHP finally.
fn collect_finally_write_paths_in_stmt(stmt: &Stmt, path: Vec<String>) -> FinallyWritePaths {
    match &stmt.kind {
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => {
            let mut condition_path = path;
            collect_expr_written_names(condition, &mut condition_path);
            let mut transfers = Vec::new();
            if !active_expr_thrown_types(condition).is_empty() {
                transfers.push(condition_path.clone());
            }
            if expr_definitely_skips_finally(condition) {
                return FinallyWritePaths {
                    continuing: Vec::new(),
                    transfers,
                };
            }
            let then_paths = collect_finally_write_paths_in_block(
                then_body,
                vec![condition_path.clone()],
            );
            transfers.extend(then_paths.transfers);
            let else_paths = collect_finally_write_paths_in_if_false_path(
                elseif_clauses,
                else_body,
                condition_path,
            );
            transfers.extend(else_paths.transfers);
            let mut continuing = then_paths.continuing;
            continuing.extend(else_paths.continuing);
            FinallyWritePaths {
                continuing,
                transfers,
            }
        }
        StmtKind::IfDef {
            then_body,
            else_body,
            ..
        } => {
            let then_paths =
                collect_finally_write_paths_in_block(then_body, vec![path.clone()]);
            let else_paths = else_body
                .as_ref()
                .map(|body| collect_finally_write_paths_in_block(body, vec![path.clone()]))
                .unwrap_or_else(|| FinallyWritePaths {
                    continuing: vec![path],
                    transfers: Vec::new(),
                });
            FinallyWritePaths {
                continuing: then_paths
                    .continuing
                    .into_iter()
                    .chain(else_paths.continuing)
                    .collect(),
                transfers: then_paths
                    .transfers
                    .into_iter()
                    .chain(else_paths.transfers)
                    .collect(),
            }
        }
        _ => {
            let mut next_path = path;
            collect_written_names(stmt, &mut next_path);
            if stmt_definitely_skips_finally(stmt) {
                return FinallyWritePaths::default();
            }
            if matches!(stmt_terminal_effect(stmt), TerminalEffect::FallsThrough) {
                FinallyWritePaths {
                    continuing: vec![next_path],
                    transfers: Vec::new(),
                }
            } else {
                FinallyWritePaths {
                    continuing: Vec::new(),
                    transfers: vec![next_path],
                }
            }
        }
    }
}

/// Walks the false side of an elseif chain without treating branch fallthrough as final entry yet.
fn collect_finally_write_paths_in_if_false_path(
    elseif_clauses: &[(Expr, Vec<Stmt>)],
    else_body: &Option<Vec<Stmt>>,
    path: Vec<String>,
) -> FinallyWritePaths {
    let Some((condition, body)) = elseif_clauses.first() else {
        return else_body
            .as_ref()
            .map(|body| collect_finally_write_paths_in_block(body, vec![path.clone()]))
            .unwrap_or_else(|| FinallyWritePaths {
                continuing: vec![path],
                transfers: Vec::new(),
            });
    };
    let mut condition_path = path;
    collect_expr_written_names(condition, &mut condition_path);
    let mut transfers = Vec::new();
    if !active_expr_thrown_types(condition).is_empty() {
        transfers.push(condition_path.clone());
    }
    if expr_definitely_skips_finally(condition) {
        return FinallyWritePaths {
            continuing: Vec::new(),
            transfers,
        };
    }
    let then_paths = collect_finally_write_paths_in_block(body, vec![condition_path.clone()]);
    transfers.extend(then_paths.transfers);
    let else_paths = collect_finally_write_paths_in_if_false_path(
        &elseif_clauses[1..],
        else_body,
        condition_path,
    );
    transfers.extend(else_paths.transfers);
    FinallyWritePaths {
        continuing: then_paths
            .continuing
            .into_iter()
            .chain(else_paths.continuing)
            .collect(),
        transfers,
    }
}

/// Returns whether a statement unconditionally terminates the process without executing finally.
fn stmt_definitely_skips_finally(stmt: &Stmt) -> bool {
    matches!(&stmt.kind, StmtKind::ExprStmt(expr) if expr_definitely_skips_finally(expr))
}

/// Returns whether an expression is an unconditional `exit`/`die`, possibly error-suppressed.
fn expr_definitely_skips_finally(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::FunctionCall { name, .. } => {
            matches!(name.as_str().to_ascii_lowercase().as_str(), "exit" | "die")
        }
        ExprKind::ErrorSuppress(inner) => expr_definitely_skips_finally(inner),
        _ => false,
    }
}
