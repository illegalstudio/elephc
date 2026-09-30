//! Purpose:
//! Home of the PHP `socket_get_status` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook is needed: the return type (`Mixed`) is fully determined by its declaration.
//! - `socket_get_status` is an alias for `stream_get_meta_data`.

builtin! {
    contract: "socket_get_status",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::StreamGetMetaData,
    ),
}