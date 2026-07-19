//! Purpose:
//! Home of the PHP `ptr_read8` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - `check` validates that the argument is a pointer type and returns `PhpType::Int`.
//! - `lower` is a thin wrapper over the shared `pointers::lower_ptr_read8` emitter.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "ptr_read8",
    area: Pointers,
    params: [pointer: Mixed],
    returns: Int,
    check: check,
    lower: lower,
    summary: "Reads one unsigned byte through a raw pointer and returns it as an integer.",
    extension: true,
}

/// Validates that the argument is a pointer type and returns `PhpType::Int`.
///
/// The registry's `check_arity` handles arity enforcement (exactly 1 argument).
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    cx.checker.ensure_pointer_type(&ty, cx.span, &format!("{}()", cx.name))?;
    Ok(PhpType::Int)
}

/// Lowers a `ptr_read8` call by dispatching to the shared pointer emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::pointers::lower_ptr_read8(ctx, inst)
}
