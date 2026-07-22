//! Purpose:
//! Home of the PHP `rename` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook: `rename` is a pure-data builtin whose `Bool` return type is
//!   fully determined by its declaration. The registry common path infers the
//!   arguments and enforces the exactly-2-argument arity before falling back to
//!   `returns`.


builtin! {
    name: "rename",
    area: Io,
    params: [from: Str, to: Str],
    returns: Bool,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Rename,
    ),
    summary: "Renames a file or directory.",
    php_manual: "function.rename",
}
