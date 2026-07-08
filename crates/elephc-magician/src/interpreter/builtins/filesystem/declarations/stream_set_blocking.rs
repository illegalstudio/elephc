//! Purpose:
//! Declarative eval registry entry for `stream_set_blocking`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem::declarations`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the stream blocking-mode helper.

eval_builtin! {
    name: "stream_set_blocking",
    area: Filesystem,
    params: [stream, enable],
    direct: Filesystem,
    values: Filesystem,
}
