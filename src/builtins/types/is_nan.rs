//! Purpose:
//! Home of the PHP `is_nan` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - Pure-data builtin with no check hook; arity and arg inference are handled by the registry common path.
//! - The parameter is named `num` (matching the PHP golden signature), not `value`.


builtin! {
    name: "is_nan",
    area: Types,
    params: [num: Float],
    returns: Bool,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::IsNan,
    ),
    summary: "Checks whether a float is NAN.",
    php_manual: "function.is-nan",
}
