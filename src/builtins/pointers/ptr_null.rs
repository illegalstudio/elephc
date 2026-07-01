//! Purpose:
//! Home of the PHP `ptr_null` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - `check` takes no arguments and returns `PhpType::Pointer(None)`.
//! - `lower` is a thin wrapper over the shared `pointers::lower_ptr_null` emitter.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "ptr_null",
    area: Pointers,
    params: [],
    arity_error: "ptr_null() takes 0 arguments",
    returns: Mixed,
    check: check,
    lower: lower,
    summary: "Returns a null raw pointer.",
}

/// Returns `PhpType::Pointer(None)` unconditionally (no arguments to validate).
///
/// The registry's `check_arity` handles arity enforcement (exactly 0 arguments).
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let _ = cx;
    Ok(PhpType::Pointer(None))
}

/// Lowers a `ptr_null` call by dispatching to the shared pointer emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::pointers::lower_ptr_null(ctx, inst)
}
