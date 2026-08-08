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
use crate::builtins::semantics::{
    internal_eir_semantics, BuiltinLoweringContext, BuiltinResultOwnership,
    LoweredBuiltinValue, NormalizedBuiltinCall,
};
use crate::errors::CompileError;
use crate::ir::{Effects, Op};
use crate::types::PhpType;

builtin! {
    name: "__elephc_normalize_callable",
    area: Pointers,
    params: [value: Mixed],
    returns: Callable,
    check: check,
    semantics: internal_eir_semantics(
        lower,
        Effects::READS_HEAP.union(Effects::ALLOC_HEAP).union(Effects::REFCOUNT_OP),
        BuiltinResultOwnership::Fresh,
    ),
    summary: "Normalizes a PHP callable into an owned runtime descriptor.",
    internal: true
}

/// Infers the source expression and exposes the owned callable result type.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    cx.checker.infer_type(&cx.args[0], cx.env)?;
    Ok(PhpType::Callable)
}

/// Lowers callable normalization to the dedicated owned-descriptor EIR primitive.
fn lower(
    ctx: &mut dyn BuiltinLoweringContext,
    call: &NormalizedBuiltinCall<'_>,
) -> Result<LoweredBuiltinValue, crate::builtins::semantics::BuiltinLoweringError> {
    Ok(ctx.emit_value(
        Op::NormalizeCallable,
        vec![call.operand(0)?],
        None,
        call.result_type.clone(),
        Op::NormalizeCallable.default_effects(),
        Some(call.span),
    ))
}
