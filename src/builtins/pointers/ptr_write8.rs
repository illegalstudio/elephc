//! Purpose:
//! Home of the PHP `ptr_write8` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - `check` validates pointer and integer value arguments and returns `PhpType::Void`.
//! - `lower` is a thin wrapper over the shared `pointers::lower_ptr_write8` emitter.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "ptr_write8",
    area: Pointers,
    params: [pointer: Mixed, value: Mixed],
    returns: Void,
    check: check,
    lower: lower,
    summary: "Writes one byte through a raw pointer.",
}

/// Validates pointer and integer value arguments and returns `PhpType::Void`.
///
/// The registry's `check_arity` handles arity enforcement (exactly 2 arguments).
/// The value argument must be an integer (byte writes do not accept pointer values).
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let ptr_ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    cx.checker.ensure_pointer_type(&ptr_ty, cx.span, &format!("{}()", cx.name))?;
    let value_ty = cx.checker.infer_type(&cx.args[1], cx.env)?;
    if value_ty != PhpType::Int {
        return Err(CompileError::new(
            cx.span,
            &format!("{}() value must be int", cx.name),
        ));
    }
    Ok(PhpType::Void)
}

/// Lowers a `ptr_write8` call by dispatching to the shared pointer emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::pointers::lower_ptr_write8(ctx, inst)
}
