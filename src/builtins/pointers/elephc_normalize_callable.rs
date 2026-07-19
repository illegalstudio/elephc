//! Purpose:
//! Declares the internal callable-normalization builtin used by native callback bridges.
//! It converts every PHP callable form into an owned runtime callable descriptor.
//!
//! Called from:
//! - The generated PDO prelude before SQLite stores a callback descriptor pointer.
//!
//! Key details:
//! - The returned `Callable` owns or retains its descriptor until ordinary PHP cleanup releases it.
//! - `internal: true` keeps this compiler primitive out of PHP-visible builtin catalogs.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "__elephc_normalize_callable",
    area: Internal,
    params: [value: Mixed],
    returns: Mixed,
    check: check,
    lower: lower,
    summary: "Normalizes a PHP callable into an owned runtime descriptor.",
    internal: true
}

/// Infers the source expression and exposes the owned callable result type.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    cx.checker.infer_type(&cx.args[0], cx.env)?;
    Ok(PhpType::Callable)
}

/// Lowers callable normalization through the shared pointer/callable emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::pointers::lower_elephc_normalize_callable(ctx, inst)
}
