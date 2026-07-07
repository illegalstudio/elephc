//! Purpose:
//! Declarative eval registry entry for `disk_total_space`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem::declarations`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the disk-space helper.

eval_builtin! {
    name: "disk_total_space",
    area: Filesystem,
    params: [directory],
    direct: Filesystem,
    values: Filesystem,
}
