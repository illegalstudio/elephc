//! Purpose:
//! Declarative eval registry entry for `stream_resolve_include_path`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem::declarations`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the include-path resolution helper.

eval_builtin! {
    name: "stream_resolve_include_path",
    area: Filesystem,
    params: [filename],
    direct: Filesystem,
    values: Filesystem,
}
