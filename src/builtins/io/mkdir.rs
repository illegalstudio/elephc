//! Purpose:
//! Home of the PHP `mkdir` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook: `mkdir` is a pure-data builtin whose `Bool` return type is
//!   fully determined by its declaration. Unlike `unlink`, `mkdir` has no PHAR
//!   side effect, so no library-linking check hook is required. The registry
//!   common path infers the argument and enforces the exactly-1-argument arity
//!   before falling back to `returns`.


builtin! {
    name: "mkdir",
    area: Io,
    params: [directory: Str],
    returns: Bool,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Mkdir,
    ),
    summary: "Makes a directory.",
    php_manual: "function.mkdir",
}
