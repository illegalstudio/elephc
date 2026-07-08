//! Purpose:
//! Declarative eval registry entry for `fwrite`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem::declarations`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the stream write helper.

eval_builtin! {
    name: "fwrite",
    area: Filesystem,
    params: [stream, data],
    direct: Filesystem,
    values: Filesystem,
}
