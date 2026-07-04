//! Purpose:
//! Home of the PHP `stream_get_wrappers` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - `check` returns `Array(Str)`, which is not scalar-expressible, so `returns: Mixed` is
//!   used and the hook overrides the return type. The hook takes no arguments.
//! - `lower` is a thin wrapper over `io::lower_stream_get_wrappers` in the EIR backend.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "stream_get_wrappers",
    area: Io,
    params: [],
    returns: Mixed,
    check: check,
    lower: lower,
    summary: "Retrieves list of registered streams.",
    php_manual: "function.stream-get-wrappers",
}

/// Returns `Array(Str)` as the precise return type for `stream_get_wrappers`.
fn check(_cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    Ok(PhpType::Array(Box::new(PhpType::Str)))
}

/// Lowers a `stream_get_wrappers` call by dispatching to the shared io emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::io::lower_stream_get_wrappers(ctx, inst)
}
