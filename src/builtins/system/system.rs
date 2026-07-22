//! Purpose:
//! Home of the PHP `system` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - Pure-data builtin: return type (`Str`) is fully determined by the declaration.


builtin! {
    name: "system",
    area: System,
    params: [command: Str],
    returns: Str,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::System,
    ),
    summary: "Executes an external program and displays the output.",
}
