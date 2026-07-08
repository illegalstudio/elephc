//! Purpose:
//! Declarative eval registry entry for `stream_context_set_default`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem::declarations`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the stream context helper.

eval_builtin! {
    name: "stream_context_set_default",
    area: Filesystem,
    params: [options],
    direct: Filesystem,
    values: Filesystem,
}
