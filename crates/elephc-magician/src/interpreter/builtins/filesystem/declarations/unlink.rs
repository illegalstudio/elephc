//! Purpose:
//! Declarative eval registry entry for `unlink`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem::declarations`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the unlink helper.

eval_builtin! {
    name: "unlink",
    area: Filesystem,
    params: [filename],
    direct: Filesystem,
    values: Filesystem,
}
