//! Purpose:
//! Home of the PHP `passthru` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook),
//!   both via `crate::builtins::registry`.
//!
//! Key details:
//! - Pure-data builtin: return type (`Void`) is fully determined by the declaration.
//! - `lower` is a thin wrapper over `system::lower_passthru` in the EIR backend.

use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "passthru",
    area: System,
    params: [command: Str],
    returns: Void,
    lower: lower,
    summary: "Executes an external program and passes its output directly.",
}

/// Lowers a `passthru` call by dispatching to the shared system emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::system::lower_passthru(ctx, inst)
}
