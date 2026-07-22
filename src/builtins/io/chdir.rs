//! Purpose:
//! Home of the PHP `chdir` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook: `chdir` is a pure-data builtin whose `Bool` return type is
//!   fully determined by its declaration. Unlike `unlink`, `chdir` has no PHAR
//!   side effect, so no library-linking check hook is required. The registry
//!   common path infers the argument and enforces the exactly-1-argument arity
//!   before falling back to `returns`.


builtin! {
    name: "chdir",
    area: Io,
    params: [directory: Str],
    returns: Bool,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Chdir,
    ),
    summary: "Changes the current directory.",
    php_manual: "function.chdir",
}
