//! Purpose:
//! Prunes constant control-flow loop exit cases.
//! Rewrites statements or expressions whose compile-time condition is known while preserving required effects.
//!
//! Called from:
//! - `crate::optimize::control::prune`
//!
//! Key details:
//! - Loop exits, empty bodies, and effectful conditions must be handled before removing structural statements.

use super::super::*;

pub(super) fn block_contains_loop_exit(body: &[Stmt]) -> bool {
    body.iter().any(stmt_contains_loop_exit)
}

fn stmt_contains_loop_exit(stmt: &Stmt) -> bool {
    match &stmt.kind {
        StmtKind::Break(_) | StmtKind::Continue(_) => true,
        StmtKind::If {
            then_body,
            elseif_clauses,
            else_body,
            ..
        } => {
            block_contains_loop_exit(then_body)
                || elseif_clauses
                    .iter()
                    .any(|(_, body)| block_contains_loop_exit(body))
                || else_body
                    .as_ref()
                    .is_some_and(|body| block_contains_loop_exit(body))
        }
        StmtKind::IfDef {
            then_body, else_body, ..
        } => {
            block_contains_loop_exit(then_body)
                || else_body
                    .as_ref()
                    .is_some_and(|body| block_contains_loop_exit(body))
        }
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            block_contains_loop_exit(try_body)
                || catches
                    .iter()
                    .any(|catch| block_contains_loop_exit(&catch.body))
                || finally_body
                    .as_ref()
                    .is_some_and(|body| block_contains_loop_exit(body))
        }
        StmtKind::Switch { cases, default, .. } => {
            cases
                .iter()
                .any(|(_, body)| block_contains_loop_exit(body))
                || default
                    .as_ref()
                    .is_some_and(|body| block_contains_loop_exit(body))
        }
        _ => false,
    }
}
