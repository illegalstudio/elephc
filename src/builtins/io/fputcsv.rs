//! Purpose:
//! Home of the PHP `fputcsv` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - `check` validates the `stream` argument is a stream resource and returns `Int`.
//! - Arguments are pre-inferred by the registry before the hook runs.
//! - `lower` is a thin wrapper over `io::lower_fputcsv` in the EIR backend.

use crate::builtins::spec::{BuiltinCheckCtx, DefaultSpec};
use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "fputcsv",
    area: Io,
    params: [
        stream: Mixed,
        fields: Mixed,
        separator: Str = DefaultSpec::Str(","),
        enclosure: Str = DefaultSpec::Str("\"")
    ],
    returns: Int,
    check: check,
    lower: lower,
    summary: "Format line as CSV and write to file pointer.",
    php_manual: "function.fputcsv",
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

/// Lowers a `fputcsv` call by dispatching to the shared io emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::io::lower_fputcsv(ctx, inst)
}
