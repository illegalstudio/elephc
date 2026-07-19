//! Purpose:
//! Home of the PHP `date_default_timezone_get` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook),
//!   both via `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook is needed: `date_default_timezone_get` is a pure-data builtin
//!   whose return type (`Str`) is fully determined by its declaration.

use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "date_default_timezone_get",
    area: System,
    params: [],
    returns: Str,
    lower: lower,
    summary: "Gets the default timezone.",
}

/// Lowers a `date_default_timezone_get` call by dispatching to the shared system emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::system::lower_date_default_timezone_get(ctx, inst)
}
