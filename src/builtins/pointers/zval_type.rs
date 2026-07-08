//! Purpose:
//! Home of the PHP `zval_type` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - `check` validates that the argument is a pointer and returns `PhpType::Int`.
//! - `lower` returns the PHP `IS_*` type byte stored in the zval.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "zval_type",
    area: Pointers,
    params: [zval: Mixed],
    returns: Int,
    check: check,
    lower: lower,
    summary: "Returns the PHP zval type byte for a zval pointer.",
}

/// Validates the zval pointer argument and returns `PhpType::Int`.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let ptr_ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    cx.checker.ensure_pointer_type(&ptr_ty, cx.span, "zval_type()")?;
    Ok(PhpType::Int)
}

/// Lowers a `zval_type` call by dispatching to the shared pointer emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::pointers::lower_zval_type(ctx, inst)
}
