//! Purpose:
//! Home of the PHP `stream_wrapper_restore` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - No check hook: the common registry path infers the protocol argument and returns `Bool`.
//! - `lower` is a thin wrapper over `io::lower_stream_wrapper_restore` in the EIR backend.

use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "stream_wrapper_restore",
    area: Io,
    params: [protocol: Str],
    returns: Bool,
    lower: lower,
    summary: "Restores a previously unregistered built-in wrapper.",
    php_manual: "function.stream-wrapper-restore",
}

/// Lowers a `stream_wrapper_restore` call by dispatching to the shared io emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::io::lower_stream_wrapper_restore(ctx, inst)
}
