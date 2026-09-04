//! Purpose:
//! Registers PHP `gc_mem_caches()` with target-neutral EIR semantics.
//!
//! Called from:
//! - The builtin registry through `crate::builtins::system`.
//!
//! Key details:
//! - elephc's fixed managed heap has no detachable allocator caches, so it reports zero bytes.

use crate::builtins::semantics::{
    callable_accepts_any_source, BuiltinArgumentLowering, BuiltinCallablePolicy,
    BuiltinEffects, BuiltinLowering, BuiltinLoweringContext, BuiltinLoweringError,
    BuiltinRequirements, BuiltinResultOwnership, BuiltinResultType, BuiltinRuntimeFunctions,
    BuiltinSemantics, BuiltinTargetStrategy, BuiltinTargetSupport, BuiltinValidation,
    LoweredBuiltinValue, NormalizedBuiltinCall,
};
use crate::ir::{GcControlOp, Immediate, Op};

builtin! {
    contract: "gc_mem_caches",
    semantics: BuiltinSemantics {
        validation: BuiltinValidation::SignatureOnly,
        result_type: BuiltinResultType::Declared,
        effects: BuiltinEffects::Static(GcControlOp::MemCaches.effects()),
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

/// Emits the allocator-cache reclamation operation and its byte count.
fn lower(
    ctx: &mut dyn BuiltinLoweringContext,
    call: &NormalizedBuiltinCall<'_>,
) -> Result<LoweredBuiltinValue, BuiltinLoweringError> {
    Ok(ctx.emit_value(
        Op::GcControl,
        Vec::new(),
        Some(Immediate::I64(GcControlOp::MemCaches.as_i64())),
        call.result_type.clone(),
        GcControlOp::MemCaches.effects(),
        Some(call.span),
    ))
}
