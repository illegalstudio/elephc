//! Purpose:
//! Home of the PHP `set_file_buffer` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook is needed: the return type (`Int`) is fully determined by its declaration.
//! - `set_file_buffer` is an alias for `stream_set_write_buffer`; both share the same runtime target.

builtin! {
    contract: "set_file_buffer",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::StreamSetWriteBuffer,
    ),
}