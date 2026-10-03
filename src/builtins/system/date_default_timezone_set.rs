//! Purpose:
//! Home of the PHP `date_default_timezone_set` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook is needed: `date_default_timezone_set` is a pure-data builtin
//!   whose return type (`Bool`) is fully determined by its declaration.


builtin! {
    contract: "date_default_timezone_set",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::DateDefaultTimezoneSet,
    ),
    requirements: crate::builtins::semantics::timezone_validation_requirements,
}
