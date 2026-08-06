//! Purpose:
//! Home of the PHP `shell_exec` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - Pure-data builtin: return type (`Str`) is fully determined by the declaration.


use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    name: "shell_exec",
    area: System,
    params: [command: Str],
    returns: Str,
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ShellExec,
    ),
    summary: "Executes a command via the shell and returns the complete output as a string.",
}

/// Refuses the call on targets whose sandbox forbids spawning a process; the
/// return type is otherwise exactly the declaration's.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    crate::builtins::spec::reject_if_process_spawn_forbidden(cx)?;
    Ok(PhpType::Str)
}
