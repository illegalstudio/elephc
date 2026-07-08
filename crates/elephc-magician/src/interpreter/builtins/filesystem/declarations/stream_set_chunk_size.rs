//! Purpose:
//! Declarative eval registry entry for `stream_set_chunk_size`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem::declarations`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the stream chunk-size metadata helper.

eval_builtin! {
    name: "stream_set_chunk_size",
    area: Filesystem,
    params: [stream, size],
    direct: Filesystem,
    values: Filesystem,
}
