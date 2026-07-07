//! Purpose:
//! Declarative eval registry entry for `rename`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem::declarations`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the binary path operation helper.

eval_builtin! {
    name: "rename",
    area: Filesystem,
    params: [from, to],
    direct: Filesystem,
    values: Filesystem,
}
