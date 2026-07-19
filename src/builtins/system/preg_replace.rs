//! Purpose:
//! Home of the PHP `preg_replace` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook),
//!   both via `crate::builtins::registry`.
//!
//! Key details:
//! - Pure-data builtin: return type (`Str`) is fully determined by the declaration.
//! - `lower` is a thin wrapper over `regex::lower_preg_replace` in the EIR backend.

use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "preg_replace",
    area: System,
    params: [pattern: Str, replacement: Str, subject: Str],
    returns: Str,
    lower: lower,
    summary: "Performs a regular expression search and replace.",
}

/// Lowers a `preg_replace` call by dispatching to the shared regex emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::regex::lower_preg_replace(ctx, inst)
}
