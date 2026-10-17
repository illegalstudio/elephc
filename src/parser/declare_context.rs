//! Purpose:
//! Tracks the leading top-level declare sequence and the main script's strict mode.
//! Keeps statement consumption in the ordinary parser, including bare declare bodies.
//!
//! Called from:
//! - `crate::parser::parse_checked_tokens()` and `crate::parser::stmt::declare`.
//!
//! Key details:
//! - Nested directives never inherit a top-level placement allowance.
//! - Main-script statements share the final strict flag; earlier callable bodies keep theirs.

use std::cell::Cell;

use crate::errors::CompileError;
use crate::lexer::SpannedToken;
use crate::parser::ast::{Stmt, StmtKind};

thread_local! {
    static STRICT_TYPES_POSITION: Cell<Option<usize>> = const { Cell::new(None) };
}

/// Restores the enclosing parser's placement allowance, including during error unwinding.
struct PositionGuard(Option<usize>);

impl Drop for PositionGuard {
    /// Restores a saved allowance when this top-level statement parse ends.
    fn drop(&mut self) {
        STRICT_TYPES_POSITION.with(|position| position.set(self.0));
    }
}

/// Parses one top-level statement with an allowance for this exact leading declare token.
pub(super) fn parse_top_level_stmt(
    tokens: &[SpannedToken],
    pos: &mut usize,
    allows_strict_types: bool,
) -> Result<Stmt, CompileError> {
    let allowed = allows_strict_types.then_some(*pos);
    let _guard = PositionGuard(STRICT_TYPES_POSITION.with(|position| position.replace(allowed)));
    super::stmt::parse_stmt(tokens, pos)
}

/// Accepts a strict directive only at the top-level token currently being parsed.
pub(super) fn allows_strict_types(pos: usize) -> bool {
    STRICT_TYPES_POSITION.with(|position| position.get() == Some(pos))
}

/// Applies the main op-array's final strict flag without changing earlier callable scopes.
pub(super) fn mark_main_strict_types(stmts: &mut [Stmt]) {
    for stmt in stmts {
        mark_main_stmt_strict_types(stmt);
    }
}

/// Visits statement bodies in the main script, stopping at function and class declarations.
fn mark_main_stmt_strict_types(stmt: &mut Stmt) {
    if matches!(stmt.kind, StmtKind::FunctionDecl { .. } | StmtKind::ClassDecl { .. }
        | StmtKind::TraitDecl { .. } | StmtKind::InterfaceDecl { .. } | StmtKind::EnumDecl { .. })
    {
        return;
    }
    stmt.strict_types = true;
    match &mut stmt.kind {
        StmtKind::Synthetic(body) | StmtKind::While { body, .. }
        | StmtKind::DoWhile { body, .. } | StmtKind::Foreach { body, .. }
        | StmtKind::NamespaceBlock { body, .. } | StmtKind::IncludeOnceGuard { body, .. } => {
            mark_main_strict_types(body);
        }
        StmtKind::If { then_body, elseif_clauses, else_body, .. } => {
            mark_main_strict_types(then_body);
            for (_, body) in elseif_clauses { mark_main_strict_types(body); }
            if let Some(body) = else_body { mark_main_strict_types(body); }
        }
        StmtKind::IfDef { then_body, else_body, .. } => {
            mark_main_strict_types(then_body);
            if let Some(body) = else_body { mark_main_strict_types(body); }
        }
        StmtKind::For { init, update, body, .. } => {
            if let Some(stmt) = init { mark_main_stmt_strict_types(stmt); }
            if let Some(stmt) = update { mark_main_stmt_strict_types(stmt); }
            mark_main_strict_types(body);
        }
        StmtKind::Switch { cases, default, .. } => {
            for (_, body) in cases { mark_main_strict_types(body); }
            if let Some(body) = default { mark_main_strict_types(body); }
        }
        StmtKind::Try { try_body, catches, finally_body } => {
            mark_main_strict_types(try_body);
            for catch in catches { mark_main_strict_types(&mut catch.body); }
            if let Some(body) = finally_body { mark_main_strict_types(body); }
        }
        // Expressions may define closures; their nested statement profiles remain lexical.
        _ => {}
    }
}
