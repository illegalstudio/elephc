//! Purpose:
//! Declarative eval registry entry for `stream_bucket_new`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem::declarations`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the stream bucket object helper.

eval_builtin! {
    name: "stream_bucket_new",
    area: Filesystem,
    params: [stream, buffer],
    direct: Filesystem,
    values: Filesystem,
}
