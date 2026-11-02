//! Purpose:
//! Answers whether a program declares a function of a given name anywhere, conditional
//! declarations included, so a prelude never injects a second body for a name the program owns.
//!
//! Called from:
//! - `crate::backend_gap_prelude::inject_if_used`.
//!
//! Key details:
//! - A polyfill declares its functions behind `if (!function_exists(...))`, so a
//!   top-level-only scan is not enough.

use crate::parser::ast::{Stmt, StmtKind};

/// Returns whether the program declares a function named `name` ANYWHERE, a conditional
/// declaration included.
///
/// A polyfill declares its functions behind `if (!function_exists(...))`; a top-level-only scan
/// missed them, both bodies were emitted, and the link failed on a duplicate symbol.
pub(crate) fn program_declares_anywhere(program: &[Stmt], name: &str) -> bool {
    program.iter().any(|stmt| stmt_declares_anywhere(stmt, name))
}

fn names_function(declared: &str, name: &str) -> bool {
    declared
        .rsplit('\\')
        .next()
        .is_some_and(|last| last.eq_ignore_ascii_case(name))
}

fn stmt_declares_anywhere(stmt: &Stmt, name: &str) -> bool {
    let any = |body: &[Stmt]| body.iter().any(|stmt| stmt_declares_anywhere(stmt, name));
    match &stmt.kind {
        StmtKind::FunctionDecl { name: declared, .. }
        | StmtKind::FunctionVariantGroup { name: declared, .. }
        | StmtKind::FunctionVariantMark { name: declared, .. } => names_function(declared, name),
        StmtKind::If { then_body, elseif_clauses, else_body, .. } => {
            any(then_body)
                || elseif_clauses.iter().any(|(_, body)| any(body))
                || else_body.as_deref().is_some_and(any)
        }
        StmtKind::IfDef { then_body, else_body, .. } => {
            any(then_body) || else_body.as_deref().is_some_and(any)
        }
        StmtKind::While { body, .. }
        | StmtKind::DoWhile { body, .. }
        | StmtKind::For { body, .. }
        | StmtKind::Foreach { body, .. }
        | StmtKind::NamespaceBlock { body, .. }
        | StmtKind::IncludeOnceGuard { body, .. }
        | StmtKind::Synthetic(body) => any(body),
        StmtKind::Switch { cases, default, .. } => {
            cases.iter().any(|(_, body)| any(body)) || default.as_deref().is_some_and(any)
        }
        StmtKind::Try { try_body, catches, finally_body } => {
            any(try_body)
                || catches.iter().any(|catch_clause| any(&catch_clause.body))
                || finally_body.as_deref().is_some_and(any)
        }
        _ => false,
    }
}
