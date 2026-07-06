//! Purpose:
//! Home of the PHP `ptr_write_string` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - `check` validates pointer and string arguments and returns `PhpType::Int`
//!   (the number of bytes written).
//! - `lower` is a thin wrapper over the shared `pointers::lower_ptr_write_string` emitter.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "ptr_write_string",
    area: Pointers,
    params: [pointer: Mixed, string: Mixed],
    returns: Int,
    check: check,
    lower: lower,
    summary: "Copies PHP string bytes into raw memory at the given pointer.",
}

/// Validates pointer and string arguments and returns `PhpType::Int`.
///
/// The registry's `check_arity` handles arity enforcement (exactly 2 arguments).
/// Returns the number of bytes written as an integer.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let ptr_ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    cx.checker.ensure_pointer_type(&ptr_ty, cx.span, "ptr_write_string()")?;
    let str_ty = cx.checker.infer_type(&cx.args[1], cx.env)?;
    if str_ty != PhpType::Str {
        return Err(CompileError::new(
            cx.span,
            "ptr_write_string() string argument must be string",
        ));
    }
    Ok(PhpType::Int)
}

/// Lowers a `ptr_write_string` call by dispatching to the shared pointer emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::pointers::lower_ptr_write_string(ctx, inst)
}
