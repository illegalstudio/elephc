//! Purpose:
//! Home of the PHP `ob_get_length` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook when present),
//!   and the EIR backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - `check` returns `Union(Int, False)`: the buffered byte count, or `false` when
//! -   no output buffer is active (runtime -1 sentinel boxed by the lowering).
//! - `lower` is a thin wrapper over `output_buffering::lower_ob_get_length`.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "ob_get_length",
    area: Io,
    params: [],
    returns: Mixed,
    returns_fresh_storage: true,
    check: check,
    lower: lower,
    summary: "Returns the length of the output buffer.",
    php_manual: "function.ob-get-length",
}

/// Returns `Union(Int, False)`: the buffered byte count, or `false` when no
/// output buffer is active.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    Ok(cx.checker.normalize_union_type(vec![PhpType::Int, PhpType::False]))
}

/// Lowers an `ob_get_length` call by dispatching to the shared output-buffering emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::output_buffering::lower_ob_get_length(ctx, inst)
}
