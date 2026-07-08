//! Purpose:
//! Declarative eval registry entry for `fread`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem::declarations`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the stream read helper.

eval_builtin! {
    name: "fread",
    area: Filesystem,
    params: [stream, length],
    direct: Filesystem,
    values: Filesystem,
}
