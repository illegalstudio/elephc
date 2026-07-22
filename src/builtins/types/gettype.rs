//! Purpose:
//! Home of the PHP `gettype` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - Pure-data builtin with no check hook; arity and arg inference are handled by the registry common path.


builtin! {
    name: "gettype",
    area: Types,
    params: [value: Mixed],
    returns: Str,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Gettype,
    ),
    summary: "Returns the type of a variable as a string.",
    php_manual: "function.gettype",
}
