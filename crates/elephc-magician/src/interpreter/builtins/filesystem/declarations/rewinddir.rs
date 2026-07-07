//! Purpose:
//! Declarative eval registry entry for `rewinddir`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem::declarations`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the directory resource rewind helper.

eval_builtin! {
    name: "rewinddir",
    area: Filesystem,
    params: [dir_handle],
    direct: Filesystem,
    values: Filesystem,
}
