//! Purpose:
//! Home of the PHP `is_file` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook is needed: `is_file` is a pure-data builtin whose return type
//!   (`Bool`) is fully determined by its declaration. The registry common path
//!   infers the argument and enforces arity before falling back to `returns`.


builtin! {
    name: "is_file",
    area: Io,
    params: [filename: Str],
    returns: Bool,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::IsFile,
    ),
    summary: "Tells whether the filename is a regular file.",
    php_manual: "function.is-file",
}
