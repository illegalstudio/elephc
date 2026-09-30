//! Purpose:
//! Home of the PHP `socket_set_blocking` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `socket_set_blocking` is an alias for `stream_set_blocking`.

builtin! {
    contract: "socket_set_blocking",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::StreamSetBlocking,
    ),
}