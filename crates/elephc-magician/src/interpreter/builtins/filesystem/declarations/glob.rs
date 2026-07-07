//! Purpose:
//! Declarative eval registry entry for `glob`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem::declarations`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the local glob helper.

eval_builtin! {
    name: "glob",
    area: Filesystem,
    params: [pattern],
    direct: Filesystem,
    values: Filesystem,
}
