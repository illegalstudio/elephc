//! Purpose:
//! Registers PHP `memory_get_peak_usage()` with target-neutral EIR semantics.
//!
//! Called from:
//! - The builtin registry through `crate::builtins::system`.
//!
//! Key details:
//! - The optional `$real_usage` flag is forwarded as a bool operand (see
//!   `memory_get_usage::real_usage_operand`); codegen picks the live-byte or arena-offset watermark.

use crate::builtins::semantics::{
    callable_accepts_any_source, BuiltinArgumentLowering, BuiltinCallablePolicy,
    BuiltinEffects, BuiltinLowering, BuiltinLoweringContext, BuiltinLoweringError,
    BuiltinRequirements, BuiltinResultOwnership, BuiltinResultType, BuiltinRuntimeFunctions,
    BuiltinSemantics, BuiltinTargetStrategy, BuiltinTargetSupport, BuiltinValidation,
    LoweredBuiltinValue, NormalizedBuiltinCall,
};
use crate::ir::{GcControlOp, Immediate, Op};

builtin! {
    contract: "memory_get_peak_usage",
    semantics: BuiltinSemantics {
        validation: BuiltinValidation::SignatureOnly,
        result_type: BuiltinResultType::Declared,
        effects: BuiltinEffects::Static(GcControlOp::MemoryPeak.effects()),
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

/// Emits the peak-usage read, forwarding the optional `$real_usage` flag to codegen.
fn lower(
    ctx: &mut dyn BuiltinLoweringContext,
    call: &NormalizedBuiltinCall<'_>,
) -> Result<LoweredBuiltinValue, BuiltinLoweringError> {
    let operands = super::memory_get_usage::real_usage_operand(ctx, call);
    Ok(ctx.emit_value(
        Op::GcControl,
        operands,
        Some(Immediate::I64(GcControlOp::MemoryPeak.as_i64())),
        call.result_type.clone(),
        GcControlOp::MemoryPeak.effects(),
        Some(call.span),
    ))
}
