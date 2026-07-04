//! Purpose:
//! Home of the PHP `stream_is_local` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - No check hook: the common registry path infers the stream argument and returns `Bool`.
//! - `lower` is a thin wrapper over `io::lower_stream_is_local` in the EIR backend.

use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "stream_is_local",
    area: Io,
    params: [stream: Mixed],
    returns: Bool,
    lower: lower,
    summary: "Checks if a stream is a local stream.",
    php_manual: "function.stream-is-local",
}

/// Lowers a `stream_is_local` call by dispatching to the shared io emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::io::lower_stream_is_local(ctx, inst)
}
