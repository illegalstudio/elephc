//! Purpose:
//! Declarative eval registry entry for `lchgrp`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem::declarations`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the ownership/group helper.

eval_builtin! {
    name: "lchgrp",
    area: Filesystem,
    params: [filename, group],
    direct: Filesystem,
    values: Filesystem,
}
