//! Purpose:
//! Home of the PHP `usleep` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook is needed: `usleep` is a pure-data builtin whose return type
//!   (`Void`) is fully determined by its declaration.


builtin! {
    name: "usleep",
    area: System,
    params: [microseconds: Int],
    returns: Void,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Usleep,
    ),
    summary: "Delays execution for a number of microseconds.",
}
