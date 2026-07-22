//! Purpose:
//! Home of the PHP `is_link` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook is needed: `is_link` is a pure-data builtin whose return type
//!   (`Bool`) is fully determined by its declaration. The registry common path
//!   infers the argument and enforces arity before falling back to `returns`.


builtin! {
    name: "is_link",
    area: Io,
    params: [filename: Str],
    returns: Bool,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::IsLink,
    ),
    summary: "Tells whether the filename is a symbolic link.",
    php_manual: "function.is-link",
}
