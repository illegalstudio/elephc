//! Purpose:
//! Home of the PHP `json_last_error_msg` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook),
//!   both via `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook is needed: `json_last_error_msg` takes no arguments and
//!   always returns `Str`. The registry common path enforces arity.

use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "json_last_error_msg",
    area: System,
    params: [],
    returns: Str,
    lower: lower,
    summary: "Returns the error string of the last json_encode() or json_decode() call.",
}

/// Lowers a `json_last_error_msg` call by dispatching to the shared JSON emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::json::lower_json_last_error_msg(ctx, inst)
}
