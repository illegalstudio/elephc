//! Purpose:
//! Home of the PHP `fwrite` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - `check` calls `ensure_stream_resource` on the stream argument for validation and
//!   returns `Int`. Arguments are pre-inferred by the registry before the hook runs.
//! - `lower` is a thin wrapper over `io::lower_fwrite` in the EIR backend.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "fwrite",
    area: Io,
    params: [stream: Mixed, data: Str],
    returns: Mixed,
    check: check,
    lower: lower,
    summary: "Binary-safe file write.",
    php_manual: "function.fwrite",
}

/// Validates the stream argument and returns PHP's `int|false` result union.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    crate::types::checker::builtins::io::common::ensure_stream_resource(
        cx.checker,
        cx.name,
        &cx.args[0],
        cx.env,
    )?;
    Ok(cx
        .checker
        .normalize_union_type(vec![PhpType::Int, PhpType::Bool]))
}

/// Lowers an `fwrite` call by dispatching to the shared io emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::io::lower_fwrite(ctx, inst)
}
