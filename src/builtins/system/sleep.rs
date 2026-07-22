//! Purpose:
//! Home of the PHP `sleep` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook is needed: `sleep` is a pure-data builtin whose return type
//!   (`Int`) is fully determined by its declaration.


builtin! {
    name: "sleep",
    area: System,
    params: [seconds: Int],
    returns: Int,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Sleep,
    ),
    summary: "Delays execution for a number of seconds.",
}
