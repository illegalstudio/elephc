//! Purpose:
//! Home of the PHP `socket_set_timeout` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook is needed: the return type (`Bool`) is fully determined by its declaration.
//! - `socket_set_timeout` is an alias for `stream_set_timeout`.


builtin! {
    contract: "socket_set_timeout",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::StreamSetTimeout,
    ),
}