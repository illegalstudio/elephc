//! Purpose:
//! Home of the PHP `passthru` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - Pure-data builtin: return type (`Void`) is fully determined by the declaration.


use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    name: "passthru",
    area: System,
    params: [command: Str],
    returns: Void,
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Passthru,
    ),
    summary: "Executes an external program and passes its output directly.",
}

/// Refuses the call on targets whose sandbox forbids spawning a process; the
/// return type is otherwise exactly the declaration's.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    crate::builtins::spec::reject_if_process_spawn_forbidden(cx)?;
    Ok(PhpType::Void)
}
