//! Purpose:
//! Declarative eval registry entry for `stream_wrapper_restore`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem::declarations`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the stream wrapper registry helper.

eval_builtin! {
    name: "stream_wrapper_restore",
    area: Filesystem,
    params: [protocol],
    direct: Filesystem,
    values: Filesystem,
}
