//! Purpose:
//! Declarative eval registry entry for `readlink`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem::declarations`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the symbolic-link target helper.

eval_builtin! {
    name: "readlink",
    area: Filesystem,
    params: [path],
    direct: Filesystem,
    values: Filesystem,
}
