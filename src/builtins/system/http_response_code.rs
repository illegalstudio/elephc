//! Purpose:
//! Home of the PHP `http_response_code` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook),
//!   both via `crate::builtins::registry`.
//!
//! Key details:
//! - Pure-data builtin: return type (`Int`) is fully determined by the declaration.
//! - `arity_error` overrides the default "takes at most 1 argument" message to match
//!   the legacy phrasing "takes 0 or 1 arguments".
//! - `lower` is a thin wrapper over `system::lower_http_response_code` in the EIR backend.

use crate::builtins::spec::DefaultSpec;
use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "http_response_code",
    area: System,
    params: [response_code: Int = DefaultSpec::Int(0)],
    arity_error: "http_response_code() takes 0 or 1 arguments",
    returns: Int,
    lower: lower,
    summary: "Gets or sets the HTTP response code.",
}

/// Lowers an `http_response_code` call by dispatching to the shared system emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::system::lower_http_response_code(ctx, inst)
}
