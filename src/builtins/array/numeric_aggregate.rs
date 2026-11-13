//! Purpose:
//! Retains explicit numeric policies for shared sum and product builtin lowering.
//!
//! Called from:
//! - The array_sum and array_product builtin homes.
//!
//! Key details:
//! - Direct and generated callable paths consume the same typed runtime inventory.
//! - Internal implementation calls retain PHP-promoting targets.

use crate::builtins::semantics::{BuiltinLowering, BuiltinLoweringContext, BuiltinLoweringError,
    BuiltinRuntimeFunctions, BuiltinSemantics, BuiltinTargetStrategy, LoweredBuiltinValue, NormalizedBuiltinCall};
use crate::ir::{RuntimeCallTarget, RuntimeFnId};
use crate::types::PhpType;

/// Declares an aggregate runtime identity once while attaching policy-aware EIR lowering.
pub(super) const fn semantics(target: RuntimeFnId) -> BuiltinSemantics {
    let mut semantics = crate::builtins::semantics::runtime_fn_semantics(target);
    semantics.lowering = BuiltinLowering::Eir(lower);
    semantics.target_strategy = BuiltinTargetStrategy::EirGraph;
    semantics
}

/// Selects a reviewed numeric aggregate target after ordinary shared argument planning.
fn lower(ctx: &mut dyn BuiltinLoweringContext, call: &NormalizedBuiltinCall<'_>)
    -> Result<LoweredBuiltinValue, BuiltinLoweringError> {
    let inventory = crate::builtins::registry::lookup(call.name)
        .map(|definition| definition.spec.semantics.runtime_functions);
    let Some(BuiltinRuntimeFunctions::One(target @ (RuntimeFnId::ArraySum | RuntimeFnId::ArrayProduct))) = inventory
        else { return Err(BuiltinLoweringError::new("numeric aggregate has no reviewed runtime target")); };
    let policy = crate::semantics::numeric::operation_policy(call.semantic_profile, call.source_mode);
    let runtime = if policy.checked_overflow() { RuntimeCallTarget::NumericFunction { target, policy } }
        else { RuntimeCallTarget::Function(target) };
    Ok(ctx.emit_runtime_call(runtime, call.operands.to_vec(), PhpType::Mixed, target.effects(), Some(call.span)))
}
