//! Purpose:
//! Home of the PHP `date` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook),
//!   both via `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook is needed: `date` is a pure-data builtin whose return type
//!   (`Str`) is fully determined by its declaration. The `timestamp` parameter
//!   is optional and defaults to `null` (current time).

use crate::builtins::spec::DefaultSpec;
use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "date",
    area: System,
    params: [format: Str, timestamp: Int = DefaultSpec::Null],
    returns: Str,
    lower: lower,
    summary: "Formats a local time/date.",
}

/// Lowers a `date` call by dispatching to the shared system emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::system::lower_date(ctx, inst)
}
