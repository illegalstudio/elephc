//! Purpose:
//! Home of the PHP `date_default_timezone_get` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook is needed: `date_default_timezone_get` is a pure-data builtin
//!   whose return type (`Str`) is fully determined by its declaration.


builtin! {
    name: "date_default_timezone_get",
    area: System,
    params: [],
    returns: Str,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::DateDefaultTimezoneGet,
    ),
    summary: "Gets the default timezone.",
}
