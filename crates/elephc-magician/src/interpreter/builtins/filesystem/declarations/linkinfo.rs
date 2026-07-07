//! Purpose:
//! Declarative eval registry entry for `linkinfo`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem::declarations`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the symbolic-link metadata helper.

eval_builtin! {
    name: "linkinfo",
    area: Filesystem,
    params: [path],
    direct: Filesystem,
    values: Filesystem,
}
