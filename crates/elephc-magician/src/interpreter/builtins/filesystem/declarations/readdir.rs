//! Purpose:
//! Declarative eval registry entry for `readdir`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem::declarations`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the directory resource read helper.

eval_builtin! {
    name: "readdir",
    area: Filesystem,
    params: [dir_handle],
    direct: Filesystem,
    values: Filesystem,
}
