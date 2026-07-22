//! Purpose:
//! Home of the PHP `is_callable` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - Pure-data builtin with no check hook; arity and arg inference are handled by the registry common path.


builtin! {
    name: "is_callable",
    area: Types,
    params: [value: Mixed],
    returns: Bool,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::IsCallable,
    ),
    summary: "Checks whether a variable can be called as a function.",
    php_manual: "function.is-callable",
}
