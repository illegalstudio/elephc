//! Purpose:
//! Home of the PHP `is_countable` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - Uses the shared typed EIR predicate: arrays are countable, and objects are countable when
//!   their runtime class implements `Countable` (checked against interface metadata).

builtin! {
    contract: "is_countable",
    semantics: crate::builtins::semantics::type_predicate_semantics(
        crate::ir::PhpTypePredicate::Countable,
    ),
}
