//! Purpose:
//! Home of the internal `__elephc_callable_is_anonymous` builtin behind
//! `ReflectionFunction::isAnonymous()` for a reflector built from a runtime Closure.
//!
//! Called from:
//! - The synthetic `ReflectionFunction::isAnonymous()` body.
//!
//! Key details:
//! - Reads the shape recorded in the Closure's descriptor: only closure literals are
//!   anonymous; `f(...)`, `$cb(...)` and `Closure::fromCallable()` Closures of a named target
//!   are not. A descriptor that cannot answer (eval adapters) throws `Error`.

use crate::builtins::semantics::{
    internal_eir_semantics, BuiltinLoweringContext, BuiltinLoweringError, BuiltinResultOwnership,
    LoweredBuiltinValue, NormalizedBuiltinCall,
};
use crate::ir::{Effects, Immediate, Op};

builtin! {
    contract: "__elephc_callable_is_anonymous",
    semantics: internal_eir_semantics(
        lower,
        Effects::READS_HEAP.union(Effects::MAY_THROW),
        BuiltinResultOwnership::NonHeap,
    ),
}

/// Lowers the anonymity read to `CallableReflectionMeta` field 0.
fn lower(
    ctx: &mut dyn BuiltinLoweringContext,
    call: &NormalizedBuiltinCall<'_>,
) -> Result<LoweredBuiltinValue, BuiltinLoweringError> {
    Ok(ctx.emit_value(
        Op::CallableReflectionMeta,
        vec![call.operand(0)?],
        Some(Immediate::I64(0)),
        call.result_type.clone(),
        Op::CallableReflectionMeta.default_effects(),
        Some(call.span),
    ))
}
