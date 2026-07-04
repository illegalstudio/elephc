//! Purpose:
//! Home of the PHP `ptr` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - `check` validates that the argument is a variable (not an arbitrary expression)
//!   and returns `PhpType::Pointer(None)`.
//! - `lower` is a thin wrapper over the shared `pointers::lower_ptr` emitter.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::parser::ast::ExprKind;
use crate::types::PhpType;

builtin! {
    name: "ptr",
    area: Pointers,
    params: [value: Mixed],
    returns: Mixed,
    check: check,
    lower: lower,
    summary: "Returns a raw pointer to the given variable.",
}

/// Validates that the argument is a variable and returns `PhpType::Pointer(None)`.
///
/// The registry's `check_arity` handles arity enforcement (exactly 1 argument).
/// `ptr()` requires a variable as its argument because taking the address of an
/// arbitrary expression has no well-defined meaning in the pointer model.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    match &cx.args[0].kind {
        ExprKind::Variable(_) => {
            cx.checker.infer_type(&cx.args[0], cx.env)?;
        }
        _ => {
            return Err(CompileError::new(
                cx.span,
                "ptr() argument must be a variable",
            ));
        }
    }
    Ok(PhpType::Pointer(None))
}

/// Lowers a `ptr` call by dispatching to the shared pointer emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::pointers::lower_ptr(ctx, inst)
}
