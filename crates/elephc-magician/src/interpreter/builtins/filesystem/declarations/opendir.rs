//! Purpose:
//! Declarative eval registry entry for `opendir`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem::declarations`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the directory resource open helper.

eval_builtin! {
    name: "opendir",
    area: Filesystem,
    params: [directory],
    direct: Filesystem,
    values: Filesystem,
}
