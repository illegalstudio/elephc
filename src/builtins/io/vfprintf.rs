//! Purpose:
//! Home of the PHP `vfprintf` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - `check` calls `ensure_stream_resource` on the stream argument for validation and
//!   returns `Int`. Arguments are pre-inferred by the registry before the hook runs.
//! - `arity_error` is overridden to preserve the legacy message suffix
//!   "(stream, format, values)" that the standard derived message omits.
//! - `lower` is a thin wrapper over `io::lower_vfprintf` in the EIR backend.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "vfprintf",
    area: Io,
    params: [stream: Mixed, format: Str, values: Mixed],
    arity_error: "vfprintf() takes exactly 3 arguments (stream, format, values)",
    returns: Int,
    check: check,
    lower: lower,
    summary: "Write a formatted string to a stream.",
    php_manual: "function.vfprintf",
}

/// Validates the stream argument is a stream resource and returns `Int`.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    crate::types::checker::builtins::io::common::ensure_stream_resource(
        cx.checker,
        cx.name,
        &cx.args[0],
        cx.env,
    )?;
    Ok(PhpType::Int)
}

/// Lowers a `vfprintf` call by dispatching to the shared io emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::io::lower_vfprintf(ctx, inst)
}
