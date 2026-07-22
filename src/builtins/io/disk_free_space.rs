//! Purpose:
//! Home of the PHP `disk_free_space` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook is needed: `disk_free_space` is a pure-data builtin whose return
//!   type (`Float`) is fully determined by its declaration. The registry common path
//!   infers the argument and enforces arity before falling back to `returns`.


builtin! {
    name: "disk_free_space",
    area: Io,
    params: [directory: Str],
    returns: Float,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::DiskFreeSpace,
    ),
    summary: "Returns available space on filesystem or disk partition.",
    php_manual: "function.disk-free-space",
}
