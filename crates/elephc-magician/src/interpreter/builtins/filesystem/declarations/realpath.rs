//! Purpose:
//! Declarative eval registry entry for `realpath`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem::declarations`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the canonical path helper.

eval_builtin! {
    name: "realpath",
    area: Filesystem,
    params: [path],
    direct: Filesystem,
    values: Filesystem,
}
