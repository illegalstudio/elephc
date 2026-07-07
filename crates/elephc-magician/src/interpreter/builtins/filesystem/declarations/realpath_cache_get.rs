//! Purpose:
//! Declarative eval registry entry for `realpath_cache_get`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem::declarations`.
//!
//! Key details:
//! - Runtime behavior stays delegated to elephc's empty realpath-cache helper.

eval_builtin! {
    name: "realpath_cache_get",
    area: Filesystem,
    params: [],
    direct: Filesystem,
    values: Filesystem,
}
