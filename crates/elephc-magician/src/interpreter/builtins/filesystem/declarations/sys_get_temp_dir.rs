//! Purpose:
//! Declarative eval registry entry for `sys_get_temp_dir`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem::declarations`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the temporary-directory helper.

eval_builtin! {
    name: "sys_get_temp_dir",
    area: Filesystem,
    params: [],
    direct: Filesystem,
    values: Filesystem,
}
