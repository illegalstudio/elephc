//! Purpose:
//! Declarative eval registry entry for `closedir`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem::declarations`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the directory resource close helper.

eval_builtin! {
    name: "closedir",
    area: Filesystem,
    params: [dir_handle],
    direct: Filesystem,
    values: Filesystem,
}
