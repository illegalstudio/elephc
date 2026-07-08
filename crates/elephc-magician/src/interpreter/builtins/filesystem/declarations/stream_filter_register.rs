//! Purpose:
//! Declarative eval registry entry for `stream_filter_register`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem::declarations`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the conservative filter registry helper.

eval_builtin! {
    name: "stream_filter_register",
    area: Filesystem,
    params: [filter_name, r#class],
    direct: Filesystem,
    values: Filesystem,
}
