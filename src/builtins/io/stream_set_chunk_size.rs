//! Purpose:
//! Home of the PHP `stream_set_chunk_size` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - No check hook: the common registry path infers both arguments and returns `Int`
//!   (the previous chunk size, or the PHP default of 8192 on failure).
//! - `lower` is a thin wrapper over `io::lower_stream_set_chunk_size` in the EIR backend.

use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "stream_set_chunk_size",
    area: Io,
    params: [stream: Mixed, size: Int],
    returns: Int,
    lower: lower,
    summary: "Sets the read chunk size on a stream.",
    php_manual: "function.stream-set-chunk-size",
}

/// Lowers a `stream_set_chunk_size` call by dispatching to the shared io emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::io::lower_stream_set_chunk_size(ctx, inst)
}
