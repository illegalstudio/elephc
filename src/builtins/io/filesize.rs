//! Purpose:
//! Home of the PHP `filesize` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook is needed: `filesize` is a pure-data builtin whose return type
//!   (`Int`) is fully determined by its declaration. The registry common path
//!   infers the argument and enforces arity before falling back to `returns`.


builtin! {
    name: "filesize",
    area: Io,
    params: [filename: Str],
    returns: Int,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Filesize,
    ),
    summary: "Gets file size.",
    php_manual: "function.filesize",
}
