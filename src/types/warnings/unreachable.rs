//! Purpose:
//! Emits checker warnings for unreachable cases.
//! Scans typed AST and checker metadata for suspicious but non-fatal program patterns.
//!
//! Called from:
//! - `crate::types::warnings`
//!
//! Key details:
//! - Warning analysis should preserve source spans and avoid rejecting programs that type checking accepted.

use crate::errors::CompileWarning;
use crate::parser::ast::{Stmt, StmtKind};
use crate::termination::stmt_guarantees_termination as shared_stmt_guarantees_termination;

/// Recursively scans nested statement lists to detect unreachable code.
///
/// Visits all control-flow structures (if/elseif/else, while, do-while, for,
/// foreach, switch, try/catch/finally, function decls, class/trait/interface
/// methods, namespace blocks) and propagates termination state downward.
/// When a terminating statement is encountered, all subsequent statements in
/// the same block are flagged as unreachable.
///
/// Uses `collect_unreachable_in_block` to flag unreachable code within each
/// block, then recurses into nested bodies of control-flow statements.
pub(super) fn collect_unreachable_recursive(stmts: &[Stmt], warnings: &mut Vec<CompileWarning>) {
    collect_unreachable_in_block(stmts, warnings);
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::If {
                then_body,
                elseif_clauses,
                else_body,
                ..
            } => {
                collect_unreachable_recursive(then_body, warnings);
                for (_, body) in elseif_clauses {
                    collect_unreachable_recursive(body, warnings);
                }
                if let Some(body) = else_body {
                    collect_unreachable_recursive(body, warnings);
                }
            }
            StmtKind::IfDef {
                then_body,
                else_body,
                ..
            } => {
                collect_unreachable_recursive(then_body, warnings);
                if let Some(body) = else_body {
                    collect_unreachable_recursive(body, warnings);
                }
            }
            StmtKind::While { body, .. }
            | StmtKind::DoWhile { body, .. }
            | StmtKind::Foreach { body, .. }
            | StmtKind::IncludeOnceGuard { body, .. } => {
                collect_unreachable_recursive(body, warnings)
            }
            StmtKind::For {
                init, update, body, ..
            } => {
                if let Some(stmt) = init {
                    collect_unreachable_recursive(std::slice::from_ref(stmt), warnings);
                }
                if let Some(stmt) = update {
                    collect_unreachable_recursive(std::slice::from_ref(stmt), warnings);
                }
                collect_unreachable_recursive(body, warnings);
            }
            StmtKind::Switch { cases, default, .. } => {
                for (_, body) in cases {
                    collect_unreachable_recursive(body, warnings);
                }
                if let Some(body) = default {
                    collect_unreachable_recursive(body, warnings);
                }
            }
            StmtKind::Try {
                try_body,
                catches,
                finally_body,
            } => {
                collect_unreachable_recursive(try_body, warnings);
                for catch_clause in catches {
                    collect_unreachable_recursive(&catch_clause.body, warnings);
                }
                if let Some(body) = finally_body {
                    collect_unreachable_recursive(body, warnings);
                }
            }
            StmtKind::FunctionDecl { body, .. } => collect_unreachable_recursive(body, warnings),
            StmtKind::ClassDecl { methods, .. }
            | StmtKind::TraitDecl { methods, .. }
            | StmtKind::InterfaceDecl { methods, .. } => {
                for method in methods {
                    collect_unreachable_recursive(&method.body, warnings);
                }
            }
            StmtKind::NamespaceBlock { body, .. } => collect_unreachable_recursive(body, warnings),
            _ => {}
        }
    }
}

/// Flags individual statements that follow a terminating statement in a linear block.
///
/// Iterates statements in order, setting `terminated = true` when a statement
/// that guarantees termination is found. Any statement visited after that point
/// generates an "Unreachable code" warning. Does not recurse into nested bodies
/// — use `collect_unreachable_recursive` for full traversal.
fn collect_unreachable_in_block(stmts: &[Stmt], warnings: &mut Vec<CompileWarning>) {
    let mut terminated = false;
    for stmt in stmts {
        if terminated {
            warnings.push(CompileWarning::new(stmt.span, "Unreachable code"));
            continue;
        }
        if stmt_guarantees_termination(stmt) {
            terminated = true;
        }
    }
}

/// Delegates to the shared `stmt_guarantees_termination` helper from the termination module.
///
/// This wrapper exists to allow unreachable analysis to call the shared logic
/// without importing the internal path directly.
fn stmt_guarantees_termination(stmt: &Stmt) -> bool {
    shared_stmt_guarantees_termination(stmt)
}
