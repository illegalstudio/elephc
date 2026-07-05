//! Purpose:
//! Home of the PHP `checkdate` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook),
//!   both via `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook is needed: `checkdate` is a pure-data builtin whose return type
//!   (`Bool`) is fully determined by its declaration.

use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "checkdate",
    area: System,
    params: [month: Int, day: Int, year: Int],
    returns: Bool,
    lower: lower,
    summary: "Validates a Gregorian date.",
}

/// Lowers a `checkdate` call by dispatching to the shared system emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::system::lower_checkdate(ctx, inst)
}
