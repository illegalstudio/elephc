//! Purpose:
//! Declarative eval registry entry for `mkdir`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem::declarations`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the unary path operation helper.

eval_builtin! {
    name: "mkdir",
    area: Filesystem,
    params: [directory],
    direct: Filesystem,
    values: Filesystem,
}
