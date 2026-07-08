//! Purpose:
//! Home of the PHP `strlen` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - `lazy_check: true` so the check hook infers the argument itself (once), matching
//!   legacy exactly-once inference without duplicate pre-inference by the common path.
//! - `check` accepts `Str`, `Mixed`, and `Union` types (PHP coerces the argument to a
//!   string per standard type-juggling rules); other types are rejected.
//! - `lower` is a thin wrapper over the shared strlen emitter.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "strlen",
    area: String,
    params: [string: Str],
    returns: Int,
    check: check,
    lazy_check: true,
    lower: lower,
    summary: "Returns the length of a string.",
    php_manual: "function.strlen",
}

/// Validates the `strlen` argument and returns `Int` for accepted string-like types.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    // Accept Str, Mixed, and Union types — PHP's strlen() coerces its
    // argument to a string per the standard PHP type juggling rules
    // (numbers become their decimal representation, true → "1",
    // false/null → ""). Mixed inputs flow through __rt_mixed_strlen
    // at codegen time which reads the cell tag and returns the
    // length of the coerced representation.
    if !matches!(ty, PhpType::Str | PhpType::Mixed | PhpType::Union(_)) {
        return Err(CompileError::new(cx.span, "strlen() argument must be string"));
    }
    Ok(PhpType::Int)
}

/// Lowers a `strlen` call by dispatching to the shared strlen emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::lower_strlen(ctx, inst)
}
