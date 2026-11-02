//! Purpose:
//! Declares PHP's builtin `ErrorException` as an AST-built class extending `Exception`.
//!
//! Called from:
//! - `crate::pipeline::compile()` in the compat-prelude phase, after autoload expansion.
//!
//! Key details:
//! - `Exception` is a fixed-layout runtime object, but a class that EXTENDS it with its own
//!   state is ordinary elephc code, which is exactly the shape php's `ErrorException` has: a
//!   `$severity` slot, a six-parameter constructor that may override `$file`/`$line`, and a
//!   final `getSeverity()`.
//! - Pay-for-use: injected when the program names the class, or when code the compiler cannot
//!   read may construct it (`eval`, a runtime `include`, `new $c`). The motivating producer is
//!   a user error handler turning a warning into `throw new \ErrorException(...)`, which
//!   Symfony's `ErrorHandler` does for every warning it is configured to throw on.
//! - A program that declares its own `ErrorException` keeps it and gets nothing injected.

use crate::parser::ast::{BinOp, Program, Stmt, StmtKind, TypeExpr};
use crate::prelude_prune::usage;
use crate::synthetic_class::{
    class, e_binop, e_const, e_int, e_null, e_parent_call, e_str, e_this, e_this_prop, e_var,
    internal_declarations, method, s_expr, s_if, s_prop_assign, t_class, t_nullable,
};

/// Prelude inventory group holding the class.
pub(crate) const GROUP: &str = "error_exception";

/// The class name this prelude declares.
const CLASS_NAME: &str = "ErrorException";

/// Builds `class ErrorException extends Exception { ... }`.
pub(crate) fn declarations() -> Program {
    internal_declarations(|| {
        let constructor = method("__construct")
            .param_default("message", TypeExpr::Str, e_str(""))
            .param_default("code", TypeExpr::Int, e_int(0))
            .param_default("severity", TypeExpr::Int, e_const("E_ERROR"))
            .param_default("filename", t_nullable(TypeExpr::Str), e_null())
            .param_default("line", t_nullable(TypeExpr::Int), e_null())
            .param_default("previous", t_nullable(t_class("Throwable")), e_null())
            .body(vec![
                s_expr(e_parent_call(
                    "__construct",
                    vec![e_var("message"), e_var("code"), e_var("previous")],
                )),
                s_prop_assign(e_this(), "severity", e_var("severity")),
                s_if(
                    e_binop(e_var("filename"), BinOp::StrictNotEq, e_null()),
                    vec![s_prop_assign(e_this(), "file", e_var("filename"))],
                    Vec::new(),
                    None,
                ),
                s_if(
                    e_binop(e_var("line"), BinOp::StrictNotEq, e_null()),
                    vec![s_prop_assign(e_this(), "line", e_var("line"))],
                    Vec::new(),
                    None,
                ),
            ]);
        let get_severity = method("getSeverity")
            .final_()
            .returns(TypeExpr::Int)
            .returning(e_this_prop("severity"));
        vec![class(CLASS_NAME)
            .extends("Exception")
            .protected_prop("severity", TypeExpr::Int, Some(e_const("E_ERROR")))
            .method(constructor)
            .method(get_severity)
            .build()]
    })
}

/// Returns whether the program may construct or name `ErrorException`.
fn program_may_reach(program: &[Stmt]) -> bool {
    let used = usage::collect(program);
    let key = crate::names::php_symbol_key(CLASS_NAME);
    used.classes.contains(&key)
        || used.literals.contains(&key)
        || used.introspects
        || used.includes_runtime_php
        || used.constructs_dynamic_class
}

/// Returns whether the program declares a class named `ErrorException` itself.
fn program_declares_class(program: &[Stmt]) -> bool {
    program.iter().any(|stmt| match &stmt.kind {
        StmtKind::ClassDecl { name, .. } => name.eq_ignore_ascii_case(CLASS_NAME),
        StmtKind::NamespaceBlock { name: None, body }
        | StmtKind::Synthetic(body)
        | StmtKind::IncludeOnceGuard { body, .. } => program_declares_class(body),
        StmtKind::If {
            then_body,
            elseif_clauses,
            else_body,
            ..
        } => {
            program_declares_class(then_body)
                || elseif_clauses
                    .iter()
                    .any(|(_, arm)| program_declares_class(arm))
                || else_body
                    .as_deref()
                    .is_some_and(program_declares_class)
        }
        _ => false,
    })
}

/// Prepends `ErrorException` when the program may reach it and does not declare its own.
pub fn inject_if_used(
    program: Program,
    inventory: &mut crate::optimize::reachability::PreludeInventory,
) -> Program {
    if !program_may_reach(&program) || program_declares_class(&program) {
        return program;
    }
    let mut combined = declarations();
    inventory.record_program(GROUP, &combined);
    combined.extend(program);
    combined
}

#[cfg(test)]
mod tests {
    //! Purpose:
    //! Unit tests for the `ErrorException` prelude gate.
    //!
    //! Called from:
    //! - `cargo test` through Rust's test harness.
    //!
    //! Key details:
    //! - Tests parse raw source, which is the stage `inject_if_used` runs at.

    use super::*;

    fn parse(source: &str) -> Program {
        let tokens = crate::lexer::tokenize(source).expect("test source must tokenize");
        crate::parser::parse(&tokens).expect("test source must parse")
    }

    fn injected(source: &str) -> bool {
        let mut inventory = crate::optimize::reachability::PreludeInventory::new();
        let before = parse(source).len();
        inject_if_used(parse(source), &mut inventory).len() != before
    }

    #[test]
    fn a_program_that_never_names_it_is_left_alone() {
        assert!(!injected("<?php echo 1;"));
    }

    #[test]
    fn naming_the_class_injects_it() {
        assert!(injected("<?php throw new ErrorException('x');"));
    }

    #[test]
    fn a_user_declaration_wins() {
        assert!(!injected(
            "<?php class ErrorException extends Exception {} throw new ErrorException('x');"
        ));
    }
}
