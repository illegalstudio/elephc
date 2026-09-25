//! Purpose:
//! Home of the PHP `mktime` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook is needed: `mktime` is a pure-data builtin whose return type
//!   (`Int`) is fully determined by its declaration.


builtin! {
    contract: "mktime",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Mktime,
    ),
    requirements: crate::builtins::semantics::windows_timezone_requirements,
}
