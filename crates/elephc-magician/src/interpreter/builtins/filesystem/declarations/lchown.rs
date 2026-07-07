//! Purpose:
//! Declarative eval registry entry for `lchown`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem::declarations`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the ownership/group helper.

eval_builtin! {
    name: "lchown",
    area: Filesystem,
    params: [filename, user],
    direct: Filesystem,
    values: Filesystem,
}
