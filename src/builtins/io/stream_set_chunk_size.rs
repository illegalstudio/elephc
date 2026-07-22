//! Purpose:
//! Home of the PHP `stream_set_chunk_size` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - No check hook: the common registry path infers both arguments and returns `Int`
//!   (the previous chunk size, or the PHP default of 8192 on failure).


builtin! {
    name: "stream_set_chunk_size",
    area: Io,
    params: [stream: Mixed, size: Int],
    returns: Int,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::StreamSetChunkSize,
    ),
    summary: "Sets the read chunk size on a stream.",
    php_manual: "function.stream-set-chunk-size",
}
