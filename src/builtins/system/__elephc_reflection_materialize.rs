//! Purpose:
//! Declares the internal builtin that builds a member reflector's declaring `ReflectionClass` on
//! demand, through the compiled materializer whose address the reflector recorded.
//!
//! Called from:
//! - The synthesized `ReflectionMethod::getDeclaringClass()` / `ReflectionProperty::getDeclaringClass()`
//!   bodies and the static `ReflectionProperty::getValue()` / `isInitialized()` branches.
//!
//! Key details:
//! - A compiled member reflector stores its declaring class's shared shallow materializer ADDRESS
//!   instead of a materialized object. Building the object eagerly made every reflector a graph
//!   that `--web` then tore down each request; Symfony's `Request::setProperty()` paid it for
//!   seven reflectors it never asks for a declaring class.
//! - Each call returns a fresh object, as php's `getDeclaringClass()` does.
//! - `internal: true` keeps this compiler primitive out of PHP-visible builtin catalogs.

use crate::builtins::semantics::{
    internal_eir_semantics, BuiltinLoweringContext, BuiltinResultOwnership,
    LoweredBuiltinValue, NormalizedBuiltinCall,
};
use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::ir::{Effects, Op};
use crate::types::PhpType;

builtin! {
    contract: "__elephc_reflection_materialize",
    check: check,
    semantics: internal_eir_semantics(
        lower,
        Effects::READS_HEAP.union(Effects::ALLOC_HEAP),
        BuiltinResultOwnership::Fresh,
    ),
}

/// Checks the materializer address and types the result as the `ReflectionClass` it builds.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    cx.checker.infer_type(&cx.args[0], cx.env)?;
    Ok(PhpType::Object("ReflectionClass".to_string()))
}

/// Lowers the call to the dedicated EIR primitive that invokes the recorded materializer.
///
/// The result is typed as the object itself: the contract's declared `mixed` return would type
/// the raw object pointer as a boxed cell, and the getter's return check then rejected it.
fn lower(
    ctx: &mut dyn BuiltinLoweringContext,
    call: &NormalizedBuiltinCall<'_>,
) -> Result<LoweredBuiltinValue, crate::builtins::semantics::BuiltinLoweringError> {
    Ok(ctx.emit_value(
        Op::ReflectionMaterialize,
        vec![call.operand(0)?],
        None,
        PhpType::Object("ReflectionClass".to_string()),
        Op::ReflectionMaterialize.default_effects(),
        Some(call.span),
    ))
}
