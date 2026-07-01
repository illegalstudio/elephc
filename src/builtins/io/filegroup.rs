//! Purpose:
//! Home of the PHP `filegroup` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - `check` returns `Union(Int, Bool)` reflecting PHP behaviour where `filegroup`
//!   returns the numeric group ID of the file owner on success or `false` on failure.
//! - The registry pre-infers arguments before calling this hook.
//! - `lower` is a thin wrapper over `io::lower_filegroup` in the EIR backend.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "filegroup",
    area: Io,
    params: [filename: Str],
    returns: Mixed,
    check: check,
    lower: lower,
    summary: "Gets file group.",
    php_manual: "function.filegroup",
}

/// Returns `Union(Int, Bool)` reflecting that `filegroup` can return a group ID or `false`.
///
/// The registry pre-infers arguments before calling this hook.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    Ok(cx.checker.normalize_union_type(vec![PhpType::Int, PhpType::Bool]))
}

/// Lowers a `filegroup` call by dispatching to the shared io emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::io::lower_filegroup(ctx, inst)
}
