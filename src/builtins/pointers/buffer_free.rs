//! Purpose:
//! Home of the `buffer_free` builtin (elephc extension): its declaration,
//! type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - `check` enforces the legacy checker-resident rules verbatim: the argument must
//!   be a plain local variable (never `$this`, a by-ref parameter, a `global`, or a
//!   `static`) of type `buffer<T>`, because lowering nulls the local slot after the
//!   free so use-after-free traps deterministically.
//! - `lower` is a thin wrapper over the shared `buffers::lower_buffer_free` emitter.
//! - `extension: true`: buffers have no PHP equivalent, so `--strict-php` hides this
//!   builtin from user programs.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::parser::ast::ExprKind;
use crate::types::PhpType;

builtin! {
    name: "buffer_free",
    area: Pointers,
    params: [buffer: Mixed],
    returns: Void,
    check: check,
    lower: lower,
    summary: "Frees a buffer<T> and nulls the local variable that held it.",
    extension: true,
}

/// Validates that the argument is a freeable local `buffer<T>` variable.
///
/// Mirrors the legacy checker arm exactly: rejects `$this`, by-ref parameters,
/// `global` and `static` variables, and non-variable expressions, then requires
/// the argument type to be `buffer<T>`. The registry's `check_arity` handles
/// arity enforcement (exactly 1 argument).
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    match &cx.args[0].kind {
        ExprKind::Variable(name) => {
            if cx.checker.current_class.is_some() && name == "this" {
                return Err(CompileError::new(cx.span, "buffer_free() cannot free $this"));
            }
            if cx.checker.active_ref_params.contains(name)
                || cx.checker.active_globals.contains(name)
                || cx.checker.active_statics.contains(name)
            {
                return Err(CompileError::new(
                    cx.span,
                    "buffer_free() argument must be a local variable",
                ));
            }
        }
        _ => {
            let ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
            if !matches!(ty, PhpType::Buffer(_)) {
                return Err(CompileError::new(
                    cx.span,
                    "buffer_free() argument must be buffer<T>",
                ));
            }
            return Err(CompileError::new(
                cx.span,
                "buffer_free() argument must be a local variable",
            ));
        }
    }
    let ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    if !matches!(ty, PhpType::Buffer(_)) {
        return Err(CompileError::new(
            cx.span,
            "buffer_free() argument must be buffer<T>",
        ));
    }
    Ok(PhpType::Void)
}

/// Lowers a `buffer_free` call by dispatching to the shared buffers emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::buffers::lower_buffer_free(ctx, inst)
}
