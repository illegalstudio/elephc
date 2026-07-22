//! Purpose:
//! Home of the PHP `gmmktime` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook is needed: `gmmktime` is a pure-data builtin whose return type
//!   (`Int`) is fully determined by its declaration.


builtin! {
    name: "gmmktime",
    area: System,
    params: [hour: Int, minute: Int, second: Int, month: Int, day: Int, year: Int],
    returns: Int,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Gmmktime,
    ),
    summary: "Returns the Unix timestamp for a GMT date.",
}
