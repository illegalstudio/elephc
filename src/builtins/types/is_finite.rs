//! Purpose:
//! Home of the PHP `is_finite` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - Pure-data builtin with no check hook; arity and arg inference are handled by the registry common path.
//! - The parameter is named `num` (matching the PHP golden signature), not `value`.


builtin! {
    name: "is_finite",
    area: Types,
    params: [num: Float],
    returns: Bool,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::IsFinite,
    ),
    summary: "Checks whether a float is finite.",
    php_manual: "function.is-finite",
}
