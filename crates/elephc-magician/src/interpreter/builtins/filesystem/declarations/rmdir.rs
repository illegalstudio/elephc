//! Purpose:
//! Declarative eval registry entry for `rmdir`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem::declarations`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the unary path operation helper.

eval_builtin! {
    name: "rmdir",
    area: Filesystem,
    params: [directory],
    direct: Filesystem,
    values: Filesystem,
}
