//! Purpose:
//! Registers PHP `gc_disable()` with target-neutral EIR semantics.
//!
//! Called from:
//! - The builtin registry through `crate::builtins::system`.
//!
//! Key details:
//! - The operation changes automatic safe points only; explicit collection remains available.

use crate::builtins::semantics::{
    callable_accepts_any_source, BuiltinArgumentLowering, BuiltinCallablePolicy,
    BuiltinEffects, BuiltinLowering, BuiltinLoweringContext, BuiltinLoweringError,
    BuiltinRequirements, BuiltinResultOwnership, BuiltinResultType, BuiltinRuntimeFunctions,
    BuiltinSemantics, BuiltinTargetStrategy, BuiltinTargetSupport, BuiltinValidation,
    LoweredBuiltinValue, NormalizedBuiltinCall,
};
use crate::ir::{GcControlOp, Immediate, Op};

builtin! {
    contract: "gc_disable",
    semantics: BuiltinSemantics {
        validation: BuiltinValidation::SignatureOnly,
        result_type: BuiltinResultType::Declared,
        effects: BuiltinEffects::Static(GcControlOp::Disable.effects()),
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

/// Emits the automatic-collection disable operation.
fn lower(
    ctx: &mut dyn BuiltinLoweringContext,
    call: &NormalizedBuiltinCall<'_>,
) -> Result<LoweredBuiltinValue, BuiltinLoweringError> {
    Ok(ctx.emit_value(
        Op::GcControl,
        Vec::new(),
        Some(Immediate::I64(GcControlOp::Disable.as_i64())),
        call.result_type.clone(),
        GcControlOp::Disable.effects(),
        Some(call.span),
    ))
}
