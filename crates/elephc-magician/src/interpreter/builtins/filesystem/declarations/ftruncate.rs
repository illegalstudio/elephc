//! Purpose:
//! Declarative eval registry entry for `ftruncate`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem::declarations`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the stream truncate helper.

eval_builtin! {
    name: "ftruncate",
    area: Filesystem,
    params: [stream, size],
    direct: Filesystem,
    values: Filesystem,
}
