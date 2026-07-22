//! Purpose:
//! Home of the PHP `stream_set_write_buffer` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - No check hook: the common registry path infers both arguments and returns `Int`
//!   (0 on success, matching PHP's successful no-op behaviour).


builtin! {
    name: "stream_set_write_buffer",
    area: Io,
    params: [stream: Mixed, size: Int],
    returns: Int,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::StreamSetWriteBuffer,
    ),
    summary: "Sets the write file buffering on a stream.",
    php_manual: "function.stream-set-write-buffer",
}
