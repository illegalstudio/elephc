//! Purpose:
//! Home of the PHP `is_numeric` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - Pure-data builtin with no check hook; arity and arg inference are handled by the registry common path.


builtin! {
    name: "is_numeric",
    area: Types,
    params: [value: Mixed],
    returns: Bool,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::IsNumeric,
    ),
    summary: "Checks whether a variable is a number or a numeric string.",
    php_manual: "function.is-numeric",
}
