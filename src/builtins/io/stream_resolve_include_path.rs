//! Purpose:
//! Home of the PHP `stream_resolve_include_path` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - No check hook: the common registry path infers the filename argument and returns `Mixed`.
//! - `returns: Mixed` reflects the `string|false` PHP return type.
//! - `lower` is a thin wrapper over `io::lower_stream_resolve_include_path` in the EIR backend.

use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "stream_resolve_include_path",
    area: Io,
    params: [filename: Str],
    returns: Mixed,
    lower: lower,
    summary: "Resolves filename against the include path.",
    php_manual: "function.stream-resolve-include-path",
}

/// Lowers a `stream_resolve_include_path` call by dispatching to the shared io emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::io::lower_stream_resolve_include_path(ctx, inst)
}
