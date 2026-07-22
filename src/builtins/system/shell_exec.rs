//! Purpose:
//! Home of the PHP `shell_exec` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - Pure-data builtin: return type (`Str`) is fully determined by the declaration.


builtin! {
    name: "shell_exec",
    area: System,
    params: [command: Str],
    returns: Str,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ShellExec,
    ),
    summary: "Executes a command via the shell and returns the complete output as a string.",
}
