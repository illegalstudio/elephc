//! Purpose:
//! Home of the PHP `diskfreespace` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `diskfreespace` is an alias for `disk_free_space`.

builtin! {
    contract: "diskfreespace",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::DiskFreeSpace,
    ),
}