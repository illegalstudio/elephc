//! Purpose:
//! Registers PHP's `is_integer` alias with the shared integer-predicate semantics.
//!
//! Called from:
//! - The builtin registry through `crate::builtins::types`.
//!
//! Key details:
//! - The alias uses the same typed EIR target as `is_int`.

builtin! {
    name: "is_integer",
    area: Types,
    params: [value: Mixed],
    returns: Bool,
    semantics: crate::builtins::semantics::type_predicate_semantics(
        crate::ir::PhpTypePredicate::Int,
    ),
    summary: "Alias of is_int().",
    php_manual: "function.is-integer",
}
