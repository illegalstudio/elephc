//! Purpose:
//! Home of the PHP `phpversion` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook),
//!   both via `crate::builtins::registry`.
//!
//! Key details:
//! - Pure-data builtin with zero parameters: return type (`Str`) is fully determined
//!   by the declaration. elephc returns the compiler package version string.
//! - `lower` delegates to the module-level `lower_phpversion` in
//!   `src/codegen_ir/lower_inst/builtins.rs`.

use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "phpversion",
    area: System,
    params: [],
    returns: Str,
    lower: lower,
    summary: "Returns the current PHP / elephc compiler version string.",
}

/// Lowers a `phpversion` call by delegating to the shared module-level emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::lower_phpversion(ctx, inst)
}
