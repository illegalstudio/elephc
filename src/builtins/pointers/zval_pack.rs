//! Purpose:
//! Home of the PHP `zval_pack` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - `check` accepts any value and returns `PhpType::Pointer(None)`.
//! - `lower` boxes the value as Mixed and dispatches to the shared zval runtime bridge.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "zval_pack",
    area: Pointers,
    params: [value: Mixed],
    returns: Mixed,
    check: check,
    lower: lower,
    summary: "Packs an elephc runtime value into a heap-allocated PHP zval pointer.",
}

/// Accepts any value and returns an untyped raw pointer to the allocated zval.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    cx.checker.infer_type(&cx.args[0], cx.env)?;
    Ok(PhpType::Pointer(None))
}

/// Lowers a `zval_pack` call by dispatching to the shared pointer emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::pointers::lower_zval_pack(ctx, inst)
}
