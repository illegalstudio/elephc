//! Purpose:
//! Home of the PHP `stream_bucket_new` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - No check hook: the common registry path infers both arguments and returns `Mixed`.
//! - `lower` is a thin wrapper over `io::lower_stream_bucket_new` in the EIR backend.

use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "stream_bucket_new",
    area: Io,
    params: [stream: Mixed, buffer: Str],
    returns: Mixed,
    lower: lower,
    summary: "Creates a new bucket for use in a stream filter.",
    php_manual: "function.stream-bucket-new",
}

/// Lowers a `stream_bucket_new` call by dispatching to the shared io emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::io::lower_stream_bucket_new(ctx, inst)
}
