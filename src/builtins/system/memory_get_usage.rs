//! Purpose:
//! Registers PHP `memory_get_usage()` with target-neutral EIR semantics.
//!
//! Called from:
//! - The builtin registry through `crate::builtins::system`.
//!
//! Key details:
//! - The optional `$real_usage` flag is forwarded as an operand, coerced to bool like PHP's
//!   parameter (a Mixed or string flag would otherwise reach codegen as a pointer); codegen picks
//!   live bytes or arena bytes.

use crate::builtins::semantics::{
    callable_accepts_any_source, BuiltinArgumentLowering, BuiltinCallablePolicy,
    BuiltinEffects, BuiltinLowering, BuiltinLoweringContext, BuiltinLoweringError,
    BuiltinRequirements, BuiltinResultOwnership, BuiltinResultType, BuiltinRuntimeFunctions,
    BuiltinSemantics, BuiltinTargetStrategy, BuiltinTargetSupport, BuiltinValidation,
    LoweredBuiltinValue, NormalizedBuiltinCall,
};
use crate::ir::{GcControlOp, Immediate, Op, ValueId};
use crate::types::PhpType;

builtin! {
    contract: "memory_get_usage",
    semantics: BuiltinSemantics {
        validation: BuiltinValidation::SignatureOnly,
        result_type: BuiltinResultType::Declared,
        effects: BuiltinEffects::Static(GcControlOp::MemoryUsage.effects()),
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

/// Emits the heap-usage read, forwarding the optional `$real_usage` flag to codegen.
fn lower(
    ctx: &mut dyn BuiltinLoweringContext,
    call: &NormalizedBuiltinCall<'_>,
) -> Result<LoweredBuiltinValue, BuiltinLoweringError> {
    let operands = real_usage_operand(ctx, call);
    Ok(ctx.emit_value(
        Op::GcControl,
        operands,
        Some(Immediate::I64(GcControlOp::MemoryUsage.as_i64())),
        call.result_type.clone(),
        GcControlOp::MemoryUsage.effects(),
        Some(call.span),
    ))
}

/// Returns the optional `$real_usage` operand as a bool, the type PHP coerces it to: a non-bool
/// argument goes through the shared truthiness predicate so codegen always sees 0 or 1.
pub(super) fn real_usage_operand(
    ctx: &mut dyn BuiltinLoweringContext,
    call: &NormalizedBuiltinCall<'_>,
) -> Vec<ValueId> {
    let Some(&flag) = call.operands.first() else {
        return Vec::new();
    };
    if matches!(ctx.value_php_type(flag), PhpType::Bool) {
        return vec![flag];
    }
    let truthy = ctx.emit_value(
        Op::IsTruthy,
        vec![flag],
        None,
        PhpType::Bool,
        Op::IsTruthy.default_effects(),
        Some(call.span),
    );
    vec![truthy.value]
}
