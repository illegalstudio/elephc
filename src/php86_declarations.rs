//! Purpose:
//! Emits PHP 8.6's declaration-time deprecations for constructors and destructors.
//!
//! Called from:
//! - `crate::pipeline::compile()` and the codegen test harness before name resolution.
//!
//! Key details:
//! - PHP reports `Returning a value from a constructor/destructor is deprecated` when a
//!   magic method returns a value, and `Making a constructor/destructor a Generator is
//!   deprecated` when it contains `yield`.
//! - The diagnostic is emitted once when the class declaration runs. elephc compiles class
//!   declarations away, so the generated program calls one helper at startup; its body is a
//!   `trigger_error(..., E_USER_DEPRECATED)` per offending method.
//! - Only declarations whose execution is unconditional (top level, namespace block,
//!   `Synthetic`, or an include-once guard) are scanned, matching the point at which the
//!   helper call runs. Nested closures and nested declarations are not entered.

use crate::parser::ast::{Program, Stmt, StmtKind};
use crate::synthetic_class::*;

/// Internal helper that emits every collected declaration deprecation.
const HELPER: &str = "__elephc_php86_declaration_deprecations";

/// Injects the deprecation helper and its startup call when the program needs them.
pub fn inject(program: Program) -> Program {
    let mut messages = Vec::new();
    collect_statements(&program, &mut messages);
    if messages.is_empty() {
        return program;
    }
    let helper = internal_declarations(|| {
        let mut body = Vec::with_capacity(messages.len());
        for message in &messages {
            body.push(s_expr(e_call(
                "\\trigger_error",
                vec![e_str(message), e_const("E_USER_DEPRECATED")],
            )));
        }
        vec![function(HELPER).body(body).build()]
    });
    let mut output = helper;
    output.push(Stmt::new(
        StmtKind::ExprStmt(e_call(&format!("\\{HELPER}"), vec![])),
        crate::span::Span::dummy(),
    ));
    output.extend(program);
    output
}

/// Collects deprecations from declarations that execute at the top level.
fn collect_statements(statements: &[Stmt], messages: &mut Vec<&'static str>) {
    for statement in statements {
        match &statement.kind {
            StmtKind::ClassDecl { methods, .. } | StmtKind::EnumDecl { methods, .. } => {
                collect_methods(methods, messages);
            }
            StmtKind::NamespaceBlock { body, .. }
            | StmtKind::Synthetic(body)
            | StmtKind::IncludeOnceGuard { body, .. } => collect_statements(body, messages),
            _ => {}
        }
    }
}

/// Collects the constructor/destructor deprecations of one class-like declaration.
fn collect_methods(methods: &[crate::parser::ast::ClassMethod], messages: &mut Vec<&'static str>) {
    for method in methods {
        let key = method.name.to_ascii_lowercase();
        let (generator, value) = match key.as_str() {
            "__construct" => (
                "Making a constructor a Generator is deprecated",
                "Returning a value from a constructor is deprecated",
            ),
            "__destruct" => (
                "Making a destructor a Generator is deprecated",
                "Returning a value from a destructor is deprecated",
            ),
            _ => continue,
        };
        if crate::types::checker::yield_validation::body_contains_yield(&method.body) {
            messages.push(generator);
        } else if body_returns_value(&method.body) {
            messages.push(value);
        }
    }
}

/// Returns whether a method body contains a `return <value>` statement.
fn body_returns_value(body: &[Stmt]) -> bool {
    body.iter().any(statement_returns_value)
}

/// Recursively checks one statement for a value return without entering nested scopes.
fn statement_returns_value(statement: &Stmt) -> bool {
    match &statement.kind {
        StmtKind::Return(Some(_)) => true,
        StmtKind::If {
            then_body,
            elseif_clauses,
            else_body,
            ..
        } => {
            then_body.iter().any(statement_returns_value)
                || elseif_clauses
                    .iter()
                    .any(|(_, body)| body.iter().any(statement_returns_value))
                || else_body
                    .as_ref()
                    .is_some_and(|body| body.iter().any(statement_returns_value))
        }
        StmtKind::While { body, .. }
        | StmtKind::DoWhile { body, .. }
        | StmtKind::For { body, .. }
        | StmtKind::Foreach { body, .. }
        | StmtKind::Synthetic(body)
        | StmtKind::IncludeOnceGuard { body, .. }
        | StmtKind::NamespaceBlock { body, .. } => body.iter().any(statement_returns_value),
        StmtKind::Switch { cases, default, .. } => {
            cases
                .iter()
                .any(|(_, body)| body.iter().any(statement_returns_value))
                || default
                    .as_ref()
                    .is_some_and(|body| body.iter().any(statement_returns_value))
        }
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            try_body.iter().any(statement_returns_value)
                || catches
                    .iter()
                    .any(|catch| catch.body.iter().any(statement_returns_value))
                || finally_body
                    .as_ref()
                    .is_some_and(|body| body.iter().any(statement_returns_value))
        }
        // A nested declaration or function has its own return scope.
        StmtKind::FunctionDecl { .. }
        | StmtKind::ClassDecl { .. }
        | StmtKind::EnumDecl { .. }
        | StmtKind::InterfaceDecl { .. }
        | StmtKind::TraitDecl { .. }
        | StmtKind::PackedClassDecl { .. }
        | StmtKind::IfDef { .. } => false,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Parses one probe source.
    fn parse(source: &str) -> Program {
        let tokens = crate::lexer::tokenize(source).expect("tokenize");
        crate::parser::parse(&tokens).expect("parse")
    }

    /// A value-returning constructor produces one deprecation message.
    #[test]
    fn value_returning_constructor_is_collected() {
        let program = parse("<?php class A { public function __construct() { return 1; } }");
        let mut messages = Vec::new();
        collect_statements(&program, &mut messages);
        assert_eq!(messages, vec!["Returning a value from a constructor is deprecated"]);
    }

    /// A generator constructor produces the generator message instead of the value one.
    #[test]
    fn generator_constructor_is_collected() {
        let program = parse("<?php class A { public function __construct() { yield 1; } }");
        let mut messages = Vec::new();
        collect_statements(&program, &mut messages);
        assert_eq!(messages, vec!["Making a constructor a Generator is deprecated"]);
    }

    /// A plain constructor in a class with other methods emits nothing.
    #[test]
    fn ordinary_constructor_is_ignored() {
        let program = parse("<?php class A { public function __construct() { } public function f(): int { return 1; } }");
        let mut messages = Vec::new();
        collect_statements(&program, &mut messages);
        assert!(messages.is_empty());
    }
}
