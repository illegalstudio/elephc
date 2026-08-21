//! Purpose:
//! Home of the PHP `socket_set_block` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook is needed: the return type (`Bool`) is fully determined by its declaration.
//! - `socket_set_block` is an alias for `stream_set_blocking`.

builtin! {
    contract: "socket_set_block",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::StreamSetBlocking,
    ),
}