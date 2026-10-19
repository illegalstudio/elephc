//! Purpose:
//! Home of the internal `__elephc_callable_scope_class` builtin behind
//! `ReflectionFunction::getClosureScopeClass()` for a reflector built from a runtime Closure.
//!
//! Called from:
//! - The synthetic `ReflectionFunction::getClosureScopeClass()` body.
//!
//! Key details:
//! - Returns the scope class recorded in the Closure's descriptor (the lexical class of a
//!   closure literal, the declaring class of a method), or `""` when the Closure has no class
//!   scope (functions, builtins, closures declared outside a class).
//! - The string lives in static descriptor data, so it never aliases the boxed Closure argument
//!   (`Independent`): the argument temporary is released after the read.
//! - A descriptor whose scope is only known per call (late-bound `static::m(...)`, eval
//!   adapters) throws `Error` rather than reporting a guess.

use crate::builtins::semantics::{
    internal_eir_semantics, BuiltinLoweringContext, BuiltinLoweringError, BuiltinResultOwnership,
    LoweredBuiltinValue, NormalizedBuiltinCall,
};
use crate::ir::{Effects, Immediate, Op};

builtin! {
    contract: "__elephc_callable_scope_class",
    semantics: internal_eir_semantics(
        lower,
        Effects::READS_HEAP.union(Effects::MAY_THROW),
        BuiltinResultOwnership::Independent,
    ),
}

/// Lowers the scope read to `CallableReflectionMeta` field 1.
fn lower(
    ctx: &mut dyn BuiltinLoweringContext,
    call: &NormalizedBuiltinCall<'_>,
) -> Result<LoweredBuiltinValue, BuiltinLoweringError> {
    Ok(ctx.emit_value(
        Op::CallableReflectionMeta,
        vec![call.operand(0)?],
        Some(Immediate::I64(1)),
        call.result_type.clone(),
        Op::CallableReflectionMeta.default_effects(),
        Some(call.span),
    ))
}
