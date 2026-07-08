//! Purpose:
//! Home of the PHP `stream_set_blocking` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - `check` validates that the first argument is a stream resource before returning `Bool`.
//! - Arguments are pre-inferred by the registry before the hook runs.
//! - `lower` is a thin wrapper over `io::lower_stream_set_blocking` in the EIR backend.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "stream_set_blocking",
    area: Io,
    params: [stream: Mixed, enable: Bool],
    returns: Bool,
    check: check,
    lower: lower,
    summary: "Sets blocking/non-blocking mode on a stream.",
    php_manual: "function.stream-set-blocking",
}

/// Validates the stream resource argument and returns `Bool`.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    crate::types::checker::builtins::io::common::ensure_stream_resource(
        cx.checker,
        cx.name,
        &cx.args[0],
        cx.env,
    )?;
    Ok(PhpType::Bool)
}

/// Lowers a `stream_set_blocking` call by dispatching to the shared io emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::io::lower_stream_set_blocking(ctx, inst)
}
