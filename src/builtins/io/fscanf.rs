//! Purpose:
//! Home of the PHP `fscanf` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - `check` calls `ensure_stream_resource` on the stream argument for validation and
//!   returns `Array<Str>` reflecting the 2-argument form that returns matched fields.
//!   `returns: Mixed` is used because `Array<Str>` cannot be expressed through the
//!   scalar `returns:` field. Arguments are pre-inferred by the registry before the
//!   hook runs.
//! - The variadic `vars` parameter is accepted but the by-ref output form is not yet
//!   supported (mirroring `sscanf()`).
//! - `lower` is a thin wrapper over `io::lower_fscanf` in the EIR backend.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "fscanf",
    area: Io,
    params: [stream: Mixed, format: Str],
    variadic: "vars",
    returns: Mixed,
    check: check,
    lower: lower,
    summary: "Parses input from a file according to a format.",
    php_manual: "function.fscanf",
}

/// Validates the stream argument and returns `Array<Str>` for the matched-fields result.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    crate::types::checker::builtins::io::common::ensure_stream_resource(
        cx.checker,
        cx.name,
        &cx.args[0],
        cx.env,
    )?;
    Ok(PhpType::Array(Box::new(PhpType::Str)))
}

/// Lowers an `fscanf` call by dispatching to the shared io emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::io::lower_fscanf(ctx, inst)
}
