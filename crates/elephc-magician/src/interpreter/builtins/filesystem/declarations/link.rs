//! Purpose:
//! Declarative eval registry entry for `link`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem::declarations`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the binary path operation helper.

eval_builtin! {
    name: "link",
    area: Filesystem,
    params: [target, link],
    direct: Filesystem,
    values: Filesystem,
}
