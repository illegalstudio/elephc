//! Purpose:
//! Declarative eval registry entry for `stream_bucket_append`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem::declarations`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the stream bucket push helper.

eval_builtin! {
    name: "stream_bucket_append",
    area: Filesystem,
    params: [brigade, bucket],
    direct: Filesystem,
    values: Filesystem,
}
