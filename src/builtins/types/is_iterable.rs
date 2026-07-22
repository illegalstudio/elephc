//! Purpose:
//! Home of the PHP `is_iterable` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - Uses the shared typed EIR predicate, including Traversable checks for object values.


builtin! {
    name: "is_iterable",
    area: Types,
    params: [value: Mixed],
    returns: Bool,
    semantics: crate::builtins::semantics::type_predicate_semantics(
        crate::ir::PhpTypePredicate::Iterable,
    ),
    summary: "Checks whether a variable is iterable.",
    php_manual: "function.is-iterable",
}
