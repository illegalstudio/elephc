//! Purpose:
//! Home of the PHP `stream_get_line` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - `check` validates that the first argument is a stream resource before returning `Str`.
//! - `ending` is optional (defaults to empty string). Arguments are pre-inferred by the registry.
//! - `lower` is a thin wrapper over `io::lower_stream_get_line` in the EIR backend.

use crate::builtins::spec::{BuiltinCheckCtx, DefaultSpec};
use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "stream_get_line",
    area: Io,
    params: [stream: Mixed, length: Int, ending: Str = DefaultSpec::Str("")],
    returns: Str,
    check: check,
    lower: lower,
    summary: "Gets line from stream resource up to a given delimiter.",
    php_manual: "function.stream-get-line",
}

/// Validates the stream resource argument and returns `Str`.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    crate::types::checker::builtins::io::common::ensure_stream_resource(
        cx.checker,
        cx.name,
        &cx.args[0],
        cx.env,
    )?;
    Ok(PhpType::Str)
}

/// Lowers a `stream_get_line` call by dispatching to the shared io emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::io::lower_stream_get_line(ctx, inst)
}
