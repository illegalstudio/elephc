//! Purpose:
//! Declarative eval registry entry for `stream_context_get_options`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem::declarations`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the stream context option reader.

eval_builtin! {
    name: "stream_context_get_options",
    area: Filesystem,
    params: [context],
    direct: Filesystem,
    values: Filesystem,
}
