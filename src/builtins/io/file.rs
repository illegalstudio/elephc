//! Purpose:
//! Home of the PHP `file` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - `check` returns `Array<Str>` (the file's lines). A check hook is required
//!   because the array return type cannot be expressed through the scalar `returns:`
//!   field.
//! - `lower` is a thin wrapper over `io::lower_file` in the EIR backend.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "file",
    area: Io,
    params: [filename: Str],
    returns: Mixed,
    check: check,
    lower: lower,
    summary: "Reads an entire file into an array.",
    php_manual: "function.file",
}

/// Returns `Array<Str>` reflecting that `file` yields the file's lines as strings.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    cx.checker.infer_type(&cx.args[0], cx.env)?;
    Ok(PhpType::Array(Box::new(PhpType::Str)))
}

/// Lowers a `file` call by dispatching to the shared io emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::io::lower_file(ctx, inst)
}
