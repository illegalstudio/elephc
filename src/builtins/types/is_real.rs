//! Purpose:
//! Registers PHP's `is_real` alias with the shared float-predicate semantics.
//!
//! Called from:
//! - The builtin registry through `crate::builtins::types`.
//!
//! Key details:
//! - The alias uses the same typed EIR target as `is_float`.

builtin! {
    name: "is_real",
    area: Types,
    params: [value: Mixed],
    returns: Bool,
    semantics: crate::builtins::semantics::type_predicate_semantics(
        crate::ir::PhpTypePredicate::Float,
    ),
    summary: "Alias of is_float().",
    php_manual: "function.is-real",
}
