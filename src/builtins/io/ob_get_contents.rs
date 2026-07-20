//! Purpose:
//! Home of the PHP `ob_get_contents` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook when present),
//!   and the EIR backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - Read-only query: the buffer stays active and untouched.
//! - `check` returns `Union(Str, False)`: the captured contents, or `false` when
//! -   no output buffer is active.
//! - `returns_fresh_storage` marks both result branches as caller-owned fresh boxes.
//! - `lower` is a thin wrapper over `output_buffering::lower_ob_get_contents`.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "ob_get_contents",
    area: Io,
    params: [],
    returns: Mixed,
    returns_fresh_storage: true,
    check: check,
    lower: lower,
    summary: "Returns the contents of the output buffer.",
    php_manual: "function.ob-get-contents",
}

/// Returns `Union(Str, False)`: the buffered bytes on success, `false` when no
/// output buffer is active.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    Ok(cx.checker.normalize_union_type(vec![PhpType::Str, PhpType::False]))
}

/// Lowers an `ob_get_contents` call by dispatching to the shared output-buffering emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::output_buffering::lower_ob_get_contents(ctx, inst)
}
