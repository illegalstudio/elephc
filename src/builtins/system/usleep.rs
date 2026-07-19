//! Purpose:
//! Home of the PHP `usleep` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook),
//!   both via `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook is needed: `usleep` is a pure-data builtin whose return type
//!   (`Void`) is fully determined by its declaration.

use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "usleep",
    area: System,
    params: [microseconds: Int],
    returns: Void,
    lower: lower,
    summary: "Delays execution for a number of microseconds.",
}

/// Lowers a `usleep` call by dispatching to the shared system emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::system::lower_usleep(ctx, inst)
}
