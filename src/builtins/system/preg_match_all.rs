//! Purpose:
//! Home of the PHP `preg_match_all` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook),
//!   both via `crate::builtins::registry`.
//!
//! Key details:
//! - Pure-data builtin: return type (`Int`) is fully determined by the declaration.
//! - `lower` is a thin wrapper over `regex::lower_preg_match_all` in the EIR backend.

use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "preg_match_all",
    area: System,
    params: [pattern: Str, subject: Str],
    returns: Int,
    lower: lower,
    summary: "Performs a global regular expression match and returns the number of matches.",
}

/// Lowers a `preg_match_all` call by dispatching to the shared regex emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::regex::lower_preg_match_all(ctx, inst)
}
