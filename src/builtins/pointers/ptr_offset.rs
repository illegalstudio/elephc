//! Purpose:
//! Home of the PHP `ptr_offset` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - `check` validates that the first argument is a pointer and the second is an
//!   integer-compatible offset, preserving the pointer's inner type annotation.
//! - `lower` is a thin wrapper over the shared `pointers::lower_ptr_offset` emitter.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "ptr_offset",
    area: Pointers,
    params: [pointer: Mixed, offset: Mixed],
    returns: Mixed,
    check: check,
    lower: lower,
    summary: "Returns a new pointer offset from the given pointer by the given byte count.",
}

/// Validates pointer and integer-compatible offset arguments and returns the pointer type.
///
/// The registry's `check_arity` handles arity enforcement (exactly 2 arguments).
/// Returns the type of the first argument (the pointer) so that pointer type annotations
/// are propagated through the offset expression.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let ptr_ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    cx.checker.ensure_pointer_type(&ptr_ty, cx.span, "ptr_offset()")?;
    let offset_ty = cx.checker.infer_type(&cx.args[1], cx.env)?;
    if !matches!(
        offset_ty,
        PhpType::Int | PhpType::Mixed | PhpType::Union(_)
    ) {
        return Err(CompileError::new(
            cx.span,
            "ptr_offset() second argument must be integer",
        ));
    }
    Ok(ptr_ty)
}

/// Lowers a `ptr_offset` call by dispatching to the shared pointer emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::pointers::lower_ptr_offset(ctx, inst)
}
