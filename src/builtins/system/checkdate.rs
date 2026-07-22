//! Purpose:
//! Home of the PHP `checkdate` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook is needed: `checkdate` is a pure-data builtin whose return type
//!   (`Bool`) is fully determined by its declaration.


builtin! {
    name: "checkdate",
    area: System,
    params: [month: Int, day: Int, year: Int],
    returns: Bool,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Checkdate,
    ),
    summary: "Validates a Gregorian date.",
}
