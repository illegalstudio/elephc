//! Purpose:
//! Home of the PHP `lstat` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - `check` returns `assoc-array<mixed, int>|bool` via `stat_result_type`, reflecting
//!   PHP behaviour where `lstat` returns the stat buffer array on success or `false` on failure.
//!   Unlike `stat`, `lstat` does not follow symbolic links.
//! - The registry pre-infers arguments before calling this hook.
//! - `lower` is a thin wrapper over `io::lower_lstat` in the EIR backend.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "lstat",
    area: Io,
    params: [filename: Str],
    returns: Mixed,
    check: check,
    lower: lower,
    summary: "Gives information about a file or symbolic link.",
    php_manual: "function.lstat",
}

/// Returns `assoc-array<mixed, int>|bool` reflecting that `lstat` returns a buffer or `false`.
///
/// The registry pre-infers arguments before calling this hook.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    Ok(crate::builtins::io::stat_support::stat_result_type(cx.checker))
}

/// Lowers an `lstat` call by dispatching to the shared io emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::io::lower_lstat(ctx, inst)
}
