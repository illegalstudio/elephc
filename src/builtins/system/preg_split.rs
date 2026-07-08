//! Purpose:
//! Home of the PHP `preg_split` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - Return element type is `Mixed` when `flags` is supplied (4 args), `Str` otherwise.
//! - `arity_error` is overridden to preserve the legacy message "preg_split() takes between
//!   2 and 4 arguments" (the registry default for min=2/max=4 produces "2 to 4 arguments").
//! - The registry pre-infers arguments before calling the hook; the hook must not
//!   call `infer_type` again.
//! - `lower` is a thin wrapper over `regex::lower_preg_split` in the EIR backend.

use crate::builtins::spec::{BuiltinCheckCtx, DefaultSpec};
use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "preg_split",
    area: System,
    params: [pattern: Str, subject: Str, limit: Int = DefaultSpec::Int(-1), flags: Int = DefaultSpec::Int(0)],
    arity_error: "preg_split() takes between 2 and 4 arguments",
    returns: Mixed,
    check: check,
    lower: lower,
    summary: "Splits a string by a regular expression.",
}

/// Returns the split result array type, refining the element type based on argument count.
///
/// Returns `Array(Mixed)` when all four arguments are present (the `flags` argument
/// can cause mixed-type entries via `PREG_OFFSET_CAPTURE`), or `Array(Str)` for 2 or
/// 3 arguments. The registry pre-infers arguments; the hook must not call `infer_type`.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let elem = if cx.args.len() >= 4 { PhpType::Mixed } else { PhpType::Str };
    Ok(PhpType::Array(Box::new(elem)))
}

/// Lowers a `preg_split` call by dispatching to the shared regex emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::regex::lower_preg_split(ctx, inst)
}
