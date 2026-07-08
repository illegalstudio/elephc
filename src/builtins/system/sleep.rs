//! Purpose:
//! Home of the PHP `sleep` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook),
//!   both via `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook is needed: `sleep` is a pure-data builtin whose return type
//!   (`Int`) is fully determined by its declaration.

use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "sleep",
    area: System,
    params: [seconds: Int],
    returns: Int,
    lower: lower,
    summary: "Delays execution for a number of seconds.",
}

/// Lowers a `sleep` call by dispatching to the shared system emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::system::lower_sleep(ctx, inst)
}
