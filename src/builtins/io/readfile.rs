//! Purpose:
//! Home of the PHP `readfile` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - `check` returns `normalize_union_type([Int, Bool])` reflecting PHP behaviour
//!   where `readfile` outputs the file and returns the byte count or `false` on
//!   failure. A check hook is required because the union return cannot be expressed
//!   through the scalar `returns:` field.
//! - `lower` is a thin wrapper over `io::lower_readfile` in the EIR backend.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "readfile",
    area: Io,
    params: [filename: Str],
    returns: Mixed,
    check: check,
    lower: lower,
    summary: "Outputs a file.",
    php_manual: "function.readfile",
}

/// Returns `Union(Int, Bool)` reflecting the byte count on success or `false` on failure.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    cx.checker.infer_type(&cx.args[0], cx.env)?;
    Ok(cx.checker.normalize_union_type(vec![PhpType::Int, PhpType::Bool]))
}

/// Lowers a `readfile` call by dispatching to the shared io emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::io::lower_readfile(ctx, inst)
}
