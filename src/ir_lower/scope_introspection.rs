//! Purpose:
//! Selects frames whose PHP scope enumeration needs actual binding presence.
//!
//! Called from:
//! - Function lowering before source initialization events are emitted.
//!
//! Key details:
//! - Deferred declarations and closure bodies belong to their own frames.
//! - Explicit call_user_func forms share the direct scope enumeration contract.

use crate::{ast_visit::{walk_statements, AstVisitor}, parser::ast::{Expr, ExprKind, Stmt, StmtKind}};

/// Identifies scope enumeration in this activation without scanning deferred bodies as its own.
pub(super) fn body_needs_binding_presence(body: &[Stmt]) -> bool {
    let mut scan = ScopeScan { deferred: 0, found: false };
    walk_statements(body, &mut scan);
    scan.found
}

/// Tracks lexical frame boundaries during the shared exhaustive AST traversal.
struct ScopeScan { deferred: usize, found: bool }

/// Separates declarations whose nested expressions execute in another frame.
fn deferred_statement(statement: &Stmt) -> bool {
    matches!(statement.kind, StmtKind::FunctionDecl { .. } | StmtKind::ClassDecl { .. }
        | StmtKind::InterfaceDecl { .. } | StmtKind::TraitDecl { .. } | StmtKind::EnumDecl { .. })
}

impl AstVisitor for ScopeScan {
    /// Starts a deferred declaration before traversing its children.
    fn statement(&mut self, statement: &Stmt) { if deferred_statement(statement) { self.deferred += 1; } }
    /// Restores the current activation after a deferred declaration.
    fn statement_end(&mut self, statement: &Stmt) { if deferred_statement(statement) { self.deferred -= 1; } }
    /// Recognizes direct and explicitly forwarded scope enumeration in the current frame.
    fn expression(&mut self, expression: &Expr) {
        if matches!(expression.kind, ExprKind::Closure { .. }) { self.deferred += 1; }
        if self.deferred != 0 { return; }
        let ExprKind::FunctionCall { name, args } = &expression.kind else { return; };
        let name = crate::names::php_symbol_key(name.trim_start_matches('\\'));
        self.found |= name == "get_defined_vars" || (matches!(name.as_str(), "call_user_func" | "call_user_func_array")
            && args.first().is_some_and(|arg| matches!(&arg.kind, ExprKind::StringLiteral(name)
                if name.trim_start_matches('\\').eq_ignore_ascii_case("get_defined_vars"))));
    }
    /// Ends a closure's deferred body without attributing its operations to its creator.
    fn expression_end(&mut self, expression: &Expr) {
        if matches!(expression.kind, ExprKind::Closure { .. }) { self.deferred -= 1; }
    }
}

/// Scope enumeration belongs to the body executing it, including explicit forwarded calls.
#[cfg(test)]
#[test]
fn scope_presence_scan_keeps_deferred_frames_separate() {
    for (source, expected) in [
        ("<?php get_defined_vars();", true),
        ("<?php call_user_func('GET_DEFINED_VARS');", true),
        ("<?php call_user_func_array('get_defined_vars', []);", true),
        ("<?php function nested():void {get_defined_vars();}", false),
        ("<?php $closure=function(){get_defined_vars();};", false),
        ("<?php class Nested {public function scope():void {get_defined_vars();}}", false),
        ("<?php strlen('get_defined_vars');", false),
    ] {
        let program = crate::parser::parse(&crate::lexer::tokenize(source).unwrap()).unwrap();
        assert_eq!(body_needs_binding_presence(&program), expected, "{source}");
    }
}
