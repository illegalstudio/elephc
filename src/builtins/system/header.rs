//! Purpose:
//! Home of the PHP `header` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook),
//!   both via `crate::builtins::registry`.
//!
//! Key details:
//! - Pure-data builtin: return type (`Void`) is fully determined by the declaration.
//! - `lower` is a thin wrapper over `system::lower_header` in the EIR backend.

use crate::builtins::spec::DefaultSpec;
use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "header",
    area: System,
    params: [header: Str, replace: Bool = DefaultSpec::Bool(true), response_code: Int = DefaultSpec::Int(0)],
    returns: Void,
    lower: lower,
    summary: "Sends a raw HTTP header.",
}

/// Lowers a `header` call by dispatching to the shared system emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::system::lower_header(ctx, inst)
}
