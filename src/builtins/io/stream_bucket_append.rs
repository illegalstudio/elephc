//! Purpose:
//! Home of the PHP `stream_bucket_append` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - No check hook: the common registry path infers both arguments and returns `Void`.
//! - `lower` dispatches to `io::lower_stream_bucket_append_or_prepend` in the EIR backend.

use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "stream_bucket_append",
    area: Io,
    params: [brigade: Mixed, bucket: Mixed],
    returns: Void,
    lower: lower,
    summary: "Appends a bucket to the brigade.",
    php_manual: "function.stream-bucket-append",
}

/// Lowers a `stream_bucket_append` call by dispatching to the shared io emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::io::lower_stream_bucket_append_or_prepend(ctx, inst)
}
