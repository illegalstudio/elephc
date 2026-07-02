//! Purpose:
//! Home of the PHP `mktime` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook),
//!   both via `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook is needed: `mktime` is a pure-data builtin whose return type
//!   (`Int`) is fully determined by its declaration.

use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "mktime",
    area: System,
    params: [hour: Int, minute: Int, second: Int, month: Int, day: Int, year: Int],
    returns: Int,
    lower: lower,
    summary: "Returns the Unix timestamp for a date.",
}

/// Lowers a `mktime` call by dispatching to the shared system emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::system::lower_mktime(ctx, inst)
}
