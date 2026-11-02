//! Purpose:
//! Exposes declarations in root groups before function, symbol and type collection.
//! Keeps ordinary lowered statement groups intact while exposing declarations.
//!
//! Called from:
//! - super::stmt::push_parsed_stmt() while building file/namespace lists.
//! - The name resolver before symbol collection for transformed programs.
//!
//! Key details:
//! - Lowered array-append groups retain their shape for checker specialization.
//! - Namespace bodies are file-level declaration contexts; expression closures are not.

use crate::parser::ast::{Program, Stmt, StmtKind};

/// Flattens declaration groups for consumers that receive a complete program.
pub(crate) fn flatten_top_level_declaration_groups(program: Program) -> Program {
    let mut flattened = Vec::with_capacity(program.len());
    for stmt in program {
        append_root_statement(&mut flattened, stmt);
    }
    flattened
}

/// Appends one file/namespace statement before declaration discovery consumes its shape.
pub(crate) fn append_root_statement(flattened: &mut Vec<Stmt>, stmt: Stmt) {
    match stmt.kind {
        StmtKind::Synthetic(body) if contains_declaration(&body) => {
            for child in body {
                append_root_statement(flattened, child);
            }
        }
        StmtKind::NamespaceBlock { name, body } => {
            flattened.push(Stmt {
                kind: StmtKind::NamespaceBlock {
                    name,
                    body: flatten_top_level_declaration_groups(body),
                },
                ..stmt
            });
        }
        kind => flattened.push(Stmt { kind, ..stmt }),
    }
}

/// Finds declaration statements without looking inside expression-owned closures.
fn contains_declaration(body: &[Stmt]) -> bool {
    body.iter().any(stmt_contains_declaration)
}

/// Checks statement bodies that can contain declarations in the surrounding PHP namespace.
fn stmt_contains_declaration(stmt: &Stmt) -> bool {
    match &stmt.kind {
        StmtKind::FunctionDecl { .. } | StmtKind::FunctionVariantGroup { .. }
        | StmtKind::ClassDecl { .. } | StmtKind::PackedClassDecl { .. }
        | StmtKind::TraitDecl { .. } | StmtKind::InterfaceDecl { .. }
        | StmtKind::EnumDecl { .. } | StmtKind::ConstDecl { .. }
        | StmtKind::NamespaceDecl { .. } | StmtKind::NamespaceBlock { .. }
        | StmtKind::UseDecl { .. } | StmtKind::ExternFunctionDecl { .. }
        | StmtKind::ExternClassDecl { .. } => true,
        StmtKind::Synthetic(body) | StmtKind::While { body, .. }
        | StmtKind::DoWhile { body, .. } | StmtKind::Foreach { body, .. }
        | StmtKind::IncludeOnceGuard { body, .. } => contains_declaration(body),
        StmtKind::If { then_body, elseif_clauses, else_body, .. } => {
            contains_declaration(then_body)
                || elseif_clauses.iter().any(|(_, body)| contains_declaration(body))
                || else_body.as_ref().is_some_and(|body| contains_declaration(body))
        }
        StmtKind::IfDef { then_body, else_body, .. } => {
            contains_declaration(then_body)
                || else_body.as_ref().is_some_and(|body| contains_declaration(body))
        }
        StmtKind::For { init, update, body, .. } => {
            init.as_ref().is_some_and(|stmt| stmt_contains_declaration(stmt))
                || update.as_ref().is_some_and(|stmt| stmt_contains_declaration(stmt))
                || contains_declaration(body)
        }
        StmtKind::Switch { cases, default, .. } => {
            cases.iter().any(|(_, body)| contains_declaration(body))
                || default.as_ref().is_some_and(|body| contains_declaration(body))
        }
        StmtKind::Try { try_body, catches, finally_body } => {
            contains_declaration(try_body)
                || catches.iter().any(|catch| contains_declaration(&catch.body))
                || finally_body.as_ref().is_some_and(|body| contains_declaration(body))
        }
        _ => false,
    }
}
