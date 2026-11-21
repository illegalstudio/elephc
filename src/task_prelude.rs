//! Purpose:
//! Builds the shared public types for the unified `Elephc\Task` API as Rust AST.
//!
//! Called from:
//! - `crate::pipeline::compile()` and matching frontend test harnesses.
//!
//! Key details:
//! - Execution selects a domain explicitly; it does not relax Parallel transfer rules.
//! - PHP source is used only by parse-parity tests, never by production injection.
//! - Strict PHP excludes this compiler-extension surface.

use crate::parser::ast::Program;
use crate::synthetic_class::{enum_decl, internal_declarations, s_namespace};

mod group;
mod handle;
mod periodic;
mod root;

/// Builds the shared public facade only for a program entering the Task root boundary.
pub(crate) fn facade_declarations() -> Program {
    internal_declarations(|| {
        let mut declarations = periodic::extern_declarations();
        declarations.push(s_namespace("Elephc\\Task", vec![
            handle::failure_declaration(), handle::task_declaration(), periodic::control_declaration(),
            group::declaration(), root::declaration(),
        ]));
        declarations
    })
}

/// Recognizes a canonical constant execution choice without evaluating or duplicating its payload.
pub(crate) fn is_parallel_execution(expr: &crate::parser::ast::Expr) -> Option<bool> {
    use crate::parser::ast::{ExprKind, StaticReceiver};
    let ExprKind::ScopedConstantAccess { receiver: StaticReceiver::Named(class), name } = &expr.kind else {
        return None;
    };
    if !class.as_str().trim_start_matches('\\').eq_ignore_ascii_case("Elephc\\Task\\Execution") {
        return None;
    }
    match name.as_str() { "Async" => Some(false), "Parallel" => Some(true), _ => None }
}

/// Builds the execution-domain enum shared by one-off and periodic submissions.
pub(crate) fn task_declarations() -> Program {
    internal_declarations(|| vec![s_namespace("Elephc\\Task", vec![
        enum_decl("Execution").case("Async").case("Parallel").build(),
    ])])
}

/// Injects the lightweight Task types before resolution and records their pruning group.
pub fn inject_types(
    program: Program,
    allow_extensions: bool,
    inventory: &mut crate::optimize::reachability::PreludeInventory,
) -> Program {
    if !allow_extensions {
        return program;
    }
    let uses_root = crate::opcache_prelude::detect::program_references_function_or_import_alias(
        &program, "Elephc\\Task\\run",
    );
    let mut declarations = task_declarations();
    if uses_root { declarations.extend(facade_declarations()); }
    inventory.record_program("task", &declarations);
    declarations.extend(program);
    declarations
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::ast::StmtKind;

    /// Verifies the Rust builder matches the public unit-enum declaration.
    #[test]
    fn task_execution_declaration_matches_php_shape() {
        let tokens = crate::lexer::tokenize(
            "<?php namespace Elephc\\Task { enum Execution { case Async; case Parallel; } }",
        ).unwrap();
        let parsed = crate::parser::parse_internal(&tokens).unwrap();
        let built = task_declarations();
        for declarations in [&built, &parsed] {
            let [namespace] = declarations.as_slice() else { panic!("one namespace required") };
            let StmtKind::NamespaceBlock { name, body } = &namespace.kind else {
                panic!("Task namespace required");
            };
            assert_eq!(name.as_ref().unwrap().as_str(), "Elephc\\Task");
            let [declaration] = body.as_slice() else { panic!("one enum required") };
            let StmtKind::EnumDecl { name, backing_type, cases, methods, .. } = &declaration.kind else {
                panic!("Execution enum required");
            };
            assert_eq!(name, "Execution");
            assert!(backing_type.is_none());
            assert!(methods.is_empty());
            assert_eq!(cases.iter().map(|case| case.name.as_str()).collect::<Vec<_>>(),
                ["Async", "Parallel"]);
            assert!(cases.iter().all(|case| case.value.is_none()));
        }
    }

    /// Strict PHP must not acquire the extension enum through automatic injection.
    #[test]
    fn task_execution_is_not_injected_in_strict_php() {
        let mut inventory = crate::optimize::reachability::PreludeInventory::new();
        assert!(inject_types(Vec::new(), false, &mut inventory).is_empty());
    }

    /// Extension mode injects one compiler-owned declaration without a runtime bridge.
    #[test]
    fn task_execution_injection_is_lightweight() {
        let mut inventory = crate::optimize::reachability::PreludeInventory::new();
        assert_eq!(inject_types(Vec::new(), true, &mut inventory).len(), 1);
    }
}
