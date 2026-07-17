//! Purpose:
//! Home of the `buffer_len` builtin (elephc extension): its declaration,
//! type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - `check` validates that the argument is a `buffer<T>` and returns `PhpType::Int`,
//!   preserving the legacy checker-resident arm's message verbatim.
//! - `lower` is a thin wrapper over the shared `buffers::lower_buffer_len` emitter.
//! - `extension: true`: buffers have no PHP equivalent, so `--strict-php` hides this
//!   builtin from user programs.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "buffer_len",
    area: Pointers,
    params: [buffer: Mixed],
    returns: Int,
    check: check,
    lower: lower,
    summary: "Returns the logical element count of a buffer<T>.",
    extension: true,
}

/// Validates that the argument is a `buffer<T>` and returns `PhpType::Int`.
///
/// The registry's `check_arity` handles arity enforcement (exactly 1 argument).
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    if !matches!(ty, PhpType::Buffer(_)) {
        return Err(CompileError::new(
            cx.span,
            "buffer_len() argument must be buffer<T>",
        ));
    }
    Ok(PhpType::Int)
}

/// Lowers a `buffer_len` call by dispatching to the shared buffers emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::buffers::lower_buffer_len(ctx, inst)
}
