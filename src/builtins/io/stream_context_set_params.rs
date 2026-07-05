//! Purpose:
//! Home of the PHP `stream_context_set_params` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - No check hook: the common registry path infers both arguments and returns `Bool`.
//! - `lower` is a thin wrapper over `io::lower_stream_context_set_params` in the EIR backend.

use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "stream_context_set_params",
    area: Io,
    params: [context: Mixed, params: Mixed],
    returns: Bool,
    lower: lower,
    summary: "Sets parameters on the specified context.",
    php_manual: "function.stream-context-set-params",
}

/// Lowers a `stream_context_set_params` call by dispatching to the shared io emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::io::lower_stream_context_set_params(ctx, inst)
}
