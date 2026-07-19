//! Purpose:
//! Home of the PHP `json_last_error` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook),
//!   both via `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook is needed: `json_last_error` takes no arguments and always
//!   returns `Int`. The registry common path enforces arity before falling back
//!   to `returns`.

use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "json_last_error",
    area: System,
    params: [],
    returns: Int,
    lower: lower,
    summary: "Returns the last error (if any) occurred during the last JSON encoding/decoding.",
}

/// Lowers a `json_last_error` call by dispatching to the shared JSON emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::json::lower_json_last_error(ctx, inst)
}
