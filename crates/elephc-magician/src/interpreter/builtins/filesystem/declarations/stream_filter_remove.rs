//! Purpose:
//! Declarative eval registry entry for `stream_filter_remove`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem::declarations`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the stream filter removal helper.

eval_builtin! {
    name: "stream_filter_remove",
    area: Filesystem,
    params: [stream_filter],
    direct: Filesystem,
    values: Filesystem,
}
