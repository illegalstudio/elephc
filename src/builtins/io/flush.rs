//! Purpose:
//! Home of the PHP `flush` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - A no-op, as `ob_implicit_flush` documents: elephc's terminal writes are unbuffered
//!   syscalls, and `--web`'s `pool` and `request` models stream each write as it is made, so
//!   there is no system write buffer left to flush. PHP's `flush()` never touches the
//!   `ob_*` buffers either.
//! - Lowers to a void constant with no runtime call, so every target supports it alike.

use crate::builtins::semantics::{
    callable_accepts_any_source, BuiltinArgumentLowering, BuiltinCallablePolicy,
    BuiltinEffects, BuiltinLowering, BuiltinLoweringContext, BuiltinLoweringError,
    BuiltinRequirements, BuiltinResultOwnership, BuiltinResultType, BuiltinRuntimeFunctions,
    BuiltinSemantics, BuiltinTargetStrategy, BuiltinTargetSupport, BuiltinValidation,
    LoweredBuiltinValue, NormalizedBuiltinCall,
};
use crate::ir::Op;

builtin! {
    contract: "flush",
    semantics: BuiltinSemantics {
        validation: BuiltinValidation::SignatureOnly,
        result_type: BuiltinResultType::Declared,
        effects: BuiltinEffects::Static(crate::ir::Effects::PURE),
        result_ownership: BuiltinResultOwnership::NonHeap,
        requirements: BuiltinRequirements::Static(&[]),
        target_strategy: BuiltinTargetStrategy::EirPrimitive,
        target_support: BuiltinTargetSupport::All,
        runtime_functions: BuiltinRuntimeFunctions::None,
        argument_lowering: BuiltinArgumentLowering::Standard,
        callable: BuiltinCallablePolicy::Dynamic(callable_accepts_any_source),
        lowering: BuiltinLowering::Eir(lower),
    },
}

/// Emits the void result: there is no buffer to flush.
fn lower(
    ctx: &mut dyn BuiltinLoweringContext,
    call: &NormalizedBuiltinCall<'_>,
) -> Result<LoweredBuiltinValue, BuiltinLoweringError> {
    Ok(ctx.emit_value(
        Op::ConstNull,
        Vec::new(),
        None,
        call.result_type.clone(),
        crate::ir::Effects::PURE,
        Some(call.span),
    ))
}
