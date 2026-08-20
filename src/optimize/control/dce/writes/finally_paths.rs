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
        if block_has_path_entering_finally(&catch.body) {
            let mut catch_writes = writes_on_paths_entering_finally(&catch.body);
            if let Some(variable) = catch.variable.as_deref() {
                catch_writes.add(variable);
            }
            written = written.union(catch_writes);
        }
    }
    let mut next = guards.clone();
    apply_guard_invalidation(&mut next, written);
    next
}

/// Path sets produced while determining which writes can precede an enclosing finally body.
#[derive(Default)]
struct FinallyWritePaths {
    continuing: Vec<Invalidation>,
    transfers: Vec<Invalidation>,
}

/// Returns the combined local invalidation on paths entering finally.
fn writes_on_paths_entering_finally(stmts: &[Stmt]) -> Invalidation {
    let paths = collect_finally_write_paths_in_block(stmts, vec![Invalidation::none()]);
    paths
        .continuing
        .into_iter()
        .chain(paths.transfers)
        .fold(Invalidation::none(), Invalidation::union)
}

/// Returns whether a block has any fallthrough/return/throw/break path that executes finally.
fn block_has_path_entering_finally(stmts: &[Stmt]) -> bool {
    let paths = collect_finally_write_paths_in_block(stmts, vec![Invalidation::none()]);
    !paths.continuing.is_empty() || !paths.transfers.is_empty()
}

/// Advances write paths through a block while separating fallthrough from finally-triggering transfers.
fn collect_finally_write_paths_in_block(
    stmts: &[Stmt],
    mut incoming: Vec<Invalidation>,
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

/// Classifies one statement's write paths, excluding successful process-exit paths.
fn collect_finally_write_paths_in_stmt(stmt: &Stmt, path: Invalidation) -> FinallyWritePaths {
    match &stmt.kind {
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => {
            let condition_path = path.union(expr_invalidation(condition));
            let mut transfers = Vec::new();
            if expr_has_path_entering_finally(condition) {
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
            let next_path = path.union(stmt_invalidation(stmt));
            if stmt_definitely_skips_finally(stmt) {
                return if !stmt_has_path_entering_finally(stmt) {
                    FinallyWritePaths::default()
                } else {
                    FinallyWritePaths {
                        continuing: Vec::new(),
                        transfers: vec![next_path],
                    }
                };
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
    path: Invalidation,
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
    let condition_path = path.union(expr_invalidation(condition));
    let mut transfers = Vec::new();
    if expr_has_path_entering_finally(condition) {
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

/// Returns whether a statement has an exceptional path that runs finally before process exit.
fn stmt_has_path_entering_finally(stmt: &Stmt) -> bool {
    match &stmt.kind {
        StmtKind::ExprStmt(expr) => expr_has_path_entering_finally(expr),
        _ => !active_stmt_thrown_types(stmt).is_empty(),
    }
}

/// Returns whether expression evaluation can throw before an unconditional `exit` or `die`.
fn expr_has_path_entering_finally(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::FunctionCall { name, args }
            if matches!(name.as_str().to_ascii_lowercase().as_str(), "exit" | "die") =>
        {
            args.iter()
                .any(|arg| !active_expr_thrown_types(arg).is_empty())
        }
        ExprKind::ErrorSuppress(inner) if expr_definitely_skips_finally(inner) => {
            expr_has_path_entering_finally(inner)
        }
        _ => !active_expr_thrown_types(expr).is_empty(),
    }
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
