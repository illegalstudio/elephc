//! Purpose:
//! Home of the PHP `zval_unpack` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - `check` validates that the argument is a pointer and returns `PhpType::Mixed`.
//! - `lower` dispatches to the runtime bridge that rebuilds an owned elephc value.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "zval_unpack",
    area: Pointers,
    params: [zval: Mixed],
    returns: Mixed,
    check: check,
    lower: lower,
    summary: "Unpacks a PHP zval pointer into an owned elephc Mixed value.",
}

/// Validates the zval pointer argument and returns `PhpType::Mixed`.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let ptr_ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    cx.checker.ensure_pointer_type(&ptr_ty, cx.span, "zval_unpack()")?;
    Ok(PhpType::Mixed)
}

/// Lowers a `zval_unpack` call by dispatching to the shared pointer emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::pointers::lower_zval_unpack(ctx, inst)
}
