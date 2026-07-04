//! Purpose:
//! Home of the PHP `stat` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - `check` returns `assoc-array<mixed, int>|bool` via `stat_result_type`, reflecting
//!   PHP behaviour where `stat` returns the stat buffer array on success or `false` on failure.
//! - The registry pre-infers arguments before calling this hook.
//! - `lower` is a thin wrapper over `io::lower_stat` in the EIR backend.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "stat",
    area: Io,
    params: [filename: Str],
    returns: Mixed,
    check: check,
    lower: lower,
    summary: "Gives information about a file.",
    php_manual: "function.stat",
}

/// Returns `assoc-array<mixed, int>|bool` reflecting that `stat` returns a buffer or `false`.
///
/// The registry pre-infers arguments before calling this hook.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    Ok(crate::builtins::io::stat_support::stat_result_type(cx.checker))
}

/// Lowers a `stat` call by dispatching to the shared io emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::io::lower_stat(ctx, inst)
}
