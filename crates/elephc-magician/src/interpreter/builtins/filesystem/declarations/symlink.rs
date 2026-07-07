//! Purpose:
//! Declarative eval registry entry for `symlink`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem::declarations`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the binary path operation helper.

eval_builtin! {
    name: "symlink",
    area: Filesystem,
    params: [target, link],
    direct: Filesystem,
    values: Filesystem,
}
