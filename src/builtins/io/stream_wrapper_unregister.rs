//! Purpose:
//! Home of the PHP `stream_wrapper_unregister` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - No check hook: the common registry path infers the protocol argument and returns `Bool`.
//! - `lower` is a thin wrapper over `io::lower_stream_wrapper_unregister` in the EIR backend.

use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "stream_wrapper_unregister",
    area: Io,
    params: [protocol: Str],
    returns: Bool,
    lower: lower,
    summary: "Unregisters a previously registered URL wrapper.",
    php_manual: "function.stream-wrapper-unregister",
}

/// Lowers a `stream_wrapper_unregister` call by dispatching to the shared io emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::io::lower_stream_wrapper_unregister(ctx, inst)
}
