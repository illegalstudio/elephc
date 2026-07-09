//! Purpose:
//! Home of the PHP `putenv` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook),
//!   both via `crate::builtins::registry`.
//!
//! Key details:
//! - Pure-data builtin: return type (`Bool`) is fully determined by the declaration.
//! - `lower` is a thin wrapper over `system::lower_putenv` in the EIR backend.

use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "putenv",
    area: System,
    params: [assignment: Str],
    returns: Bool,
    lower: lower,
    summary: "Sets an environment variable.",
}

/// Lowers a `putenv` call by dispatching to the shared system emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::system::lower_putenv(ctx, inst)
}
