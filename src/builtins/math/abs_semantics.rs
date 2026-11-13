//! Purpose:
//! Carries abs invocation policies into typed runtime calls and normalizes numeric result storage.
//!
//! Called from:
//! - The abs builtin home after shared argument planning and checker validation.
//!
//! Key details:
//! - Internal calls retain the ordinary PHP runtime target.
//! - Checked calls retain explicit operation policy through EIR optimization.
//! - Physical Mixed results are unboxed and retired before exposing a concrete scalar contract.

use crate::builtins::semantics::{
    BuiltinEffects, BuiltinLowering, BuiltinLoweringContext, BuiltinLoweringError, BuiltinSemanticInput,
    BuiltinSemantics, BuiltinTargetStrategy,
    LoweredBuiltinValue, NormalizedBuiltinCall,
};
use crate::ir::{Effects, Op, RuntimeCallTarget, RuntimeFnId};
use elephc_semantics::arithmetic_policy::ArithmeticPolicy;
use crate::types::PhpType;

/// Declares the runtime inventory once while selecting policy-aware EIR lowering.
pub(super) const fn semantics() -> BuiltinSemantics {
    let mut semantics = crate::builtins::semantics::runtime_fn_semantics(RuntimeFnId::Abs);
    semantics.lowering = BuiltinLowering::Eir(lower);
    semantics.effects = BuiltinEffects::Shared(effects);
    semantics.target_strategy = BuiltinTargetStrategy::EirGraph;
    semantics
}

/// Preserves exceptional cleanup and allocation effects for every checked invocation path.
fn effects(input: &BuiltinSemanticInput<'_>) -> Effects {
    numeric_effects(crate::semantics::numeric::operation_policy(input.semantic_profile, input.source_mode))
}

/// Projects the reviewed runtime kernel effects without changing ordinary PHP calls.
fn numeric_effects(policy: ArithmeticPolicy) -> Effects {
    if policy.checked_overflow() {
        Op::ThrowArithmeticError.default_effects() | Effects::READS_HEAP | Effects::ALLOC_HEAP | Effects::REFCOUNT_OP
    } else { RuntimeFnId::Abs.effects() }
}

/// Keeps physical numeric boxes independent of the checker-facing scalar result contract.
fn lower(
    ctx: &mut dyn BuiltinLoweringContext, call: &NormalizedBuiltinCall<'_>,
) -> Result<LoweredBuiltinValue, BuiltinLoweringError> {
    let policy = crate::semantics::numeric::operation_policy(call.semantic_profile, call.source_mode);
    let target = if policy.checked_overflow() {
        RuntimeCallTarget::NumericFunction { target: RuntimeFnId::Abs, policy }
    } else { RuntimeCallTarget::Function(RuntimeFnId::Abs) };
    let boxed = policy.checked_overflow()
        || matches!(ctx.value_php_type(call.operand(0)?).codegen_repr(), PhpType::Mixed | PhpType::Union(_));
    let result_type = if boxed { PhpType::Mixed } else { call.result_type.clone() };
    let result = ctx.emit_runtime_call(target, call.operands.to_vec(), result_type,
        numeric_effects(policy), Some(call.span));
    Ok(crate::builtins::semantics::normalize_numeric_result(ctx, result, call.result_type, call.span))
}
