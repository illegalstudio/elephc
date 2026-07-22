//! Purpose:
//! Home of the PHP `is_writeable` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook is needed: `is_writeable` is a pure-data builtin whose return
//!   type (`Bool`) is fully determined by its declaration. The registry common path
//!   infers the argument and enforces arity before falling back to `returns`.
//! - `is_writeable` is an alias for `is_writable`; both share the same lowering.


builtin! {
    name: "is_writeable",
    area: Io,
    params: [filename: Str],
    returns: Bool,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::IsWriteable,
    ),
    summary: "Tells whether the filename is writable (alias of is_writable).",
    php_manual: "function.is-writable",
}
