//! Purpose:
//! Home of the PHP `zval_free` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - `check` validates that the argument is a pointer and returns `PhpType::Void`.
//! - `lower` releases the zval allocation and any zval-owned children.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "zval_free",
    area: Pointers,
    params: [zval: Mixed],
    returns: Void,
    check: check,
    lower: lower,
    summary: "Frees a PHP zval pointer allocated by `zval_pack`.",
    extension: true,
}

/// Validates the zval pointer argument and returns `PhpType::Void`.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let ptr_ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    cx.checker.ensure_pointer_type(&ptr_ty, cx.span, "zval_free()")?;
    Ok(PhpType::Void)
}

/// Lowers a `zval_free` call by dispatching to the shared pointer emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::pointers::lower_zval_free(ctx, inst)
}
