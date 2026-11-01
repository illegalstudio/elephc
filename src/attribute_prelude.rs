//! Purpose:
//! Declares PHP's builtin `Attribute` class as an AST-built final class.
//!
//! Called from:
//! - `crate::pipeline::compile()` in the compat-prelude phase, after autoload expansion.
//!
//! Key details:
//! - php's `Attribute` carries the `TARGET_*` and `IS_REPEATABLE` constants every attribute
//!   class declaration names (`#[\Attribute(\Attribute::TARGET_CLASS | \Attribute::IS_REPEATABLE)]`),
//!   a public `$flags` and a constructor defaulting to `TARGET_ALL`. Without the class those
//!   constants were undefined -- in compiled code (`Undefined constant Attribute::TARGET_CLASS`)
//!   and in eval, where materializing an attribute's arguments failed outright. Symfony's
//!   `AttributeAutoconfigurationPass` reflects a configurator whose parameter is typed with
//!   `Routing\Attribute\Route`, a class interpreted at run time, and every `debug:config` died there.
//! - Values are php 8.5's: `TARGET_CONSTANT` (64) is new in 8.5 and `TARGET_ALL` includes it.
//! - Pay-for-use, gated like `ErrorException`: injected when the program names the class or when
//!   code the compiler cannot read may reach it. A program declaring its own `Attribute` keeps it.

use crate::parser::ast::{Program, Stmt, StmtKind, TypeExpr};
use crate::prelude_prune::usage;
use crate::synthetic_class::{
    class, e_class_const, e_int, e_this, e_var, internal_declarations, method, s_prop_assign,
};

/// Prelude inventory group holding the class.
pub(crate) const GROUP: &str = "attribute";

/// The class name this prelude declares.
const CLASS_NAME: &str = "Attribute";

/// php's `Attribute` constants, in declaration order.
const CONSTANTS: [(&str, i64); 9] = [
    ("TARGET_CLASS", 1),
    ("TARGET_FUNCTION", 2),
    ("TARGET_METHOD", 4),
    ("TARGET_PROPERTY", 8),
    ("TARGET_CLASS_CONSTANT", 16),
    ("TARGET_PARAMETER", 32),
    ("TARGET_CONSTANT", 64),
    ("TARGET_ALL", 127),
    ("IS_REPEATABLE", 128),
];

/// Builds `final class Attribute { const ...; public int $flags; __construct(...) }`.
pub(crate) fn declarations() -> Program {
    internal_declarations(|| {
        let constructor = method("__construct")
            .param_default("flags", TypeExpr::Int, e_class_const(CLASS_NAME, "TARGET_ALL"))
            .body(vec![s_prop_assign(e_this(), "flags", e_var("flags"))]);
        let mut builder = class(CLASS_NAME).final_();
        for (name, value) in CONSTANTS {
            builder = builder.constant(name, e_int(value));
        }
        vec![builder
            .prop("flags", TypeExpr::Int, None)
            .method(constructor)
            .build()]
    })
}

/// Returns whether the program may name or reach `Attribute`.
fn program_may_reach(program: &[Stmt]) -> bool {
    let used = usage::collect(program);
    let key = crate::names::php_symbol_key(CLASS_NAME);
    used.classes.contains(&key)
        || used.literals.contains(&key)
        || used.introspects
        || used.includes_runtime_php
        || used.constructs_dynamic_class
}

/// Returns whether the program declares a class named `Attribute` itself.
fn program_declares_class(program: &[Stmt]) -> bool {
    program.iter().any(|stmt| match &stmt.kind {
        StmtKind::ClassDecl { name, .. } => name.eq_ignore_ascii_case(CLASS_NAME),
        StmtKind::NamespaceBlock { name: None, body }
        | StmtKind::Synthetic(body)
        | StmtKind::IncludeOnceGuard { body, .. } => program_declares_class(body),
        _ => false,
    })
}

/// Prepends `Attribute` when the program may reach it and does not declare its own.
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
    //! Unit tests for the `Attribute` prelude gate.
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
    fn naming_a_constant_injects_it() {
        assert!(injected("<?php echo \\Attribute::TARGET_CLASS;"));
    }

    #[test]
    fn a_program_declaring_its_own_keeps_it() {
        assert!(!injected("<?php class Attribute {} echo Attribute::class;"));
    }
}
