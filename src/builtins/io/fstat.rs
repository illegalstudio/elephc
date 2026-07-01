//! Purpose:
//! Home of the PHP `fstat` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - `check` validates the `stream` argument is a stream resource via
//!   `ensure_stream_resource`, then returns `assoc-array<mixed, int>|bool` via
//!   `stat_result_type`. PHP's `fstat` returns the stat buffer array on success or
//!   `false` on failure.
//! - `ensure_stream_resource` is kept in `common.rs` (not moved) because
//!   `streams.rs` also uses it; it is widened to `pub(crate)` for access here.
//! - The registry pre-infers arguments before calling this hook (idempotent with
//!   the infer call inside `ensure_stream_resource`).
//! - `lower` is a thin wrapper over `io::lower_fstat` in the EIR backend.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "fstat",
    area: Io,
    params: [stream: Mixed],
    returns: Mixed,
    check: check,
    lower: lower,
    summary: "Gets information about a file using an open file pointer.",
    php_manual: "function.fstat",
}

/// Validates `stream` is a stream resource and returns `assoc-array<mixed, int>|bool`.
///
/// Calls `ensure_stream_resource` to emit a type error if the argument is not a
/// compatible stream type, then returns the stat result type via `stat_result_type`.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    crate::types::checker::builtins::io::common::ensure_stream_resource(
        cx.checker,
        cx.name,
        &cx.args[0],
        cx.env,
    )?;
    Ok(crate::builtins::io::stat_support::stat_result_type(cx.checker))
}

/// Lowers an `fstat` call by dispatching to the shared io emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::io::lower_fstat(ctx, inst)
}
