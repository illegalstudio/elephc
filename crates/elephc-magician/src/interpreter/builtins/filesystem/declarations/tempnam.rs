//! Purpose:
//! Declarative eval registry entry for `tempnam`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem::declarations`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the temporary-name helper.

eval_builtin! {
    name: "tempnam",
    area: Filesystem,
    params: [directory, prefix],
    direct: Filesystem,
    values: Filesystem,
}
