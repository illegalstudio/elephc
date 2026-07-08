//! Purpose:
//! Declarative eval registry entry for `stream_set_read_buffer`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem::declarations`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the stream buffer-setting helper.

eval_builtin! {
    name: "stream_set_read_buffer",
    area: Filesystem,
    params: [stream, size],
    direct: Filesystem,
    values: Filesystem,
}
