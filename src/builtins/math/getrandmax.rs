//! Purpose:
//! Registers PHP's `getrandmax` builtin as the platform-independent RAND_MAX contract.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - PHP exposes `2147483647` for the rand implementation used by supported elephc targets.

use crate::builtins::semantics::{
    BuiltinCallablePolicy, BuiltinEffects, BuiltinLowering, BuiltinLoweringContext,
    BuiltinLoweringError, BuiltinRequirements, BuiltinResultOwnership, BuiltinResultType,
    BuiltinRuntimeFunctions, BuiltinSemantics, BuiltinTargetStrategy, BuiltinTargetSupport,
    BuiltinValidation, LoweredBuiltinValue, NormalizedBuiltinCall,
};
use crate::ir::{Effects, Immediate, Op};

builtin! {
    contract: "getrandmax",
    semantics: BuiltinSemantics {
        validation: BuiltinValidation::SignatureOnly,
        result_type: BuiltinResultType::Declared,
        effects: BuiltinEffects::Static(Effects::PURE),
        result_ownership: BuiltinResultOwnership::NonHeap,
        requirements: BuiltinRequirements::Static(&[]),
        target_strategy: BuiltinTargetStrategy::EirPrimitive,
        target_support: BuiltinTargetSupport::All,
        runtime_functions: BuiltinRuntimeFunctions::None,
        argument_lowering: crate::builtins::semantics::BuiltinArgumentLowering::Standard,
        callable: BuiltinCallablePolicy::StaticOnly(
            "getrandmax is folded through its direct and first-class static call paths",
        ),
        lowering: BuiltinLowering::Eir(lower),
    },
}

/// Emits PHP's target-independent RAND_MAX integer constant.
fn lower(
    ctx: &mut dyn BuiltinLoweringContext,
    call: &NormalizedBuiltinCall<'_>,
) -> Result<LoweredBuiltinValue, BuiltinLoweringError> {
    Ok(ctx.emit_value(
        Op::ConstI64,
        Vec::new(),
        Some(Immediate::I64(i32::MAX as i64)),
        call.result_type.clone(),
        Effects::PURE,
        Some(call.span),
    ))
}
