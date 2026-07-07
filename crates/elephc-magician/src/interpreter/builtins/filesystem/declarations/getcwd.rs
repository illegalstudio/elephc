//! Purpose:
//! Declarative eval registry entry for `getcwd`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem::declarations`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the current-working-directory helper.

eval_builtin! {
    name: "getcwd",
    area: Filesystem,
    params: [],
    direct: Filesystem,
    values: Filesystem,
}
