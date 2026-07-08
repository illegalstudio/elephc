//! Purpose:
//! Declarative eval registry entry for `stream_bucket_make_writeable`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem::declarations`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the stream bucket read helper.

eval_builtin! {
    name: "stream_bucket_make_writeable",
    area: Filesystem,
    params: [brigade],
    direct: Filesystem,
    values: Filesystem,
}
