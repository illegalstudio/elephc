//! Purpose:
//! Home of the PHP `shell_exec` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook),
//!   both via `crate::builtins::registry`.
//!
//! Key details:
//! - Pure-data builtin: return type (`Str`) is fully determined by the declaration.
//! - `lower` is a thin wrapper over `system::lower_shell_exec` in the EIR backend.

use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "shell_exec",
    area: System,
    params: [command: Str],
    returns: Str,
    lower: lower,
    summary: "Executes a command via the shell and returns the complete output as a string.",
}

/// Lowers a `shell_exec` call by dispatching to the shared system emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::system::lower_shell_exec(ctx, inst)
}
