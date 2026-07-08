//! Purpose:
//! Declarative eval registry entry for `vfprintf`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem::declarations`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the vprintf-family stream write helper.

eval_builtin! {
    name: "vfprintf",
    area: Filesystem,
    params: [stream, format, values],
    direct: Filesystem,
    values: Filesystem,
}
