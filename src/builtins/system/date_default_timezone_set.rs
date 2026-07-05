//! Purpose:
//! Home of the PHP `date_default_timezone_set` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook),
//!   both via `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook is needed: `date_default_timezone_set` is a pure-data builtin
//!   whose return type (`Bool`) is fully determined by its declaration.

use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "date_default_timezone_set",
    area: System,
    params: [timezoneId: Str],
    returns: Bool,
    lower: lower,
    summary: "Sets the default timezone.",
}

/// Lowers a `date_default_timezone_set` call by dispatching to the shared system emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::system::lower_date_default_timezone_set(ctx, inst)
}
