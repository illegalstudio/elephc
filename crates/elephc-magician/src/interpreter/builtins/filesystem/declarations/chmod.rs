//! Purpose:
//! Declarative eval registry entry for `chmod`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem::declarations`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the chmod helper.

eval_builtin! {
    name: "chmod",
    area: Filesystem,
    params: [filename, permissions],
    direct: Filesystem,
    values: Filesystem,
}
