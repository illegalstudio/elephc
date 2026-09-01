//! Purpose:
//! Registers PHP's `gc_collect_cycles` builtin as an explicit native collector safe point.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - The EIR opcode reports the number of heap blocks reclaimed by the collection pass.

use crate::builtins::semantics::{
    BuiltinCallablePolicy, BuiltinEffects, BuiltinLowering, BuiltinLoweringContext,
    BuiltinLoweringError, BuiltinRequirements, BuiltinResultOwnership, BuiltinResultType,
    BuiltinRuntimeFunctions, BuiltinSemantics, BuiltinTargetStrategy, BuiltinTargetSupport,
    BuiltinValidation, LoweredBuiltinValue, NormalizedBuiltinCall,
};
use crate::ir::Op;

builtin! {
    contract: "gc_collect_cycles",
    semantics: BuiltinSemantics {
        validation: BuiltinValidation::SignatureOnly,
        result_type: BuiltinResultType::Declared,
        effects: BuiltinEffects::Shared(effects),
        result_ownership: BuiltinResultOwnership::NonHeap,
        requirements: BuiltinRequirements::Static(&[]),
        target_strategy: BuiltinTargetStrategy::EirPrimitive,
        target_support: BuiltinTargetSupport::All,
        runtime_functions: BuiltinRuntimeFunctions::None,
        argument_lowering: crate::builtins::semantics::BuiltinArgumentLowering::Standard,
        callable: BuiltinCallablePolicy::StaticOnly(
            "gc_collect_cycles is emitted as an explicit native ownership safe point",
        ),
        lowering: BuiltinLowering::Eir(lower),
    },
}

/// Returns the collector opcode's conservative heap-mutation and refcount effects.
fn effects(_input: &crate::builtins::semantics::BuiltinSemanticInput<'_>) -> crate::ir::Effects {
    Op::GcCollect.default_effects()
}

/// Emits the value-producing collector opcode used by the direct AOT call path.
fn lower(
    ctx: &mut dyn BuiltinLoweringContext,
    call: &NormalizedBuiltinCall<'_>,
) -> Result<LoweredBuiltinValue, BuiltinLoweringError> {
    Ok(ctx.emit_value(
        Op::GcCollect,
        Vec::new(),
        None,
        call.result_type.clone(),
        Op::GcCollect.default_effects(),
        Some(call.span),
    ))
}
