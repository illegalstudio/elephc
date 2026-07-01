//! Purpose:
//! Home of the PHP `stream_set_write_buffer` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - No check hook: the common registry path infers both arguments and returns `Int`
//!   (0 on success, matching PHP's successful no-op behaviour).
//! - `lower` dispatches to `io::lower_stream_set_buffer`, shared with `stream_set_read_buffer`.

use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "stream_set_write_buffer",
    area: Io,
    params: [stream: Mixed, size: Int],
    returns: Int,
    lower: lower,
    summary: "Sets the write file buffering on a stream.",
    php_manual: "function.stream-set-write-buffer",
}

/// Lowers a `stream_set_write_buffer` call by dispatching to the shared io buffer emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::io::lower_stream_set_buffer(ctx, inst)
}
