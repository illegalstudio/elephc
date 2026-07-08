//! Purpose:
//! Declarative eval registry entry for `stream_isatty`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem::declarations`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the stream descriptor predicate helper.

eval_builtin! {
    name: "stream_isatty",
    area: Filesystem,
    params: [stream],
    direct: Filesystem,
    values: Filesystem,
}
