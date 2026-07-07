//! Purpose:
//! Declarative eval registry entry for `scandir`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem::declarations`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the directory listing helper.

eval_builtin! {
    name: "scandir",
    area: Filesystem,
    params: [directory],
    direct: Filesystem,
    values: Filesystem,
}
