//! Purpose:
//! Home of the PHP `ptr_read_string` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - `check` validates that the first argument is a pointer and the second is an integer
//!   length, and returns `PhpType::Str`.
//! - `lower` is a thin wrapper over the shared `pointers::lower_ptr_read_string` emitter.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "ptr_read_string",
    area: Pointers,
    params: [pointer: Mixed, length: Mixed],
    returns: Str,
    check: check,
    lower: lower,
    summary: "Copies raw bytes from a pointer into a PHP string of the given length.",
}

/// Validates pointer and integer length arguments and returns `PhpType::Str`.
///
/// The registry's `check_arity` handles arity enforcement (exactly 2 arguments).
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let ptr_ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    cx.checker.ensure_pointer_type(&ptr_ty, cx.span, "ptr_read_string()")?;
    let len_ty = cx.checker.infer_type(&cx.args[1], cx.env)?;
    if len_ty != PhpType::Int {
        return Err(CompileError::new(
            cx.span,
            "ptr_read_string() length must be int",
        ));
    }
    Ok(PhpType::Str)
}

/// Lowers a `ptr_read_string` call by dispatching to the shared pointer emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::pointers::lower_ptr_read_string(ctx, inst)
}
