//! Purpose:
//! Checks builtin comparison modes after shared source argument evaluation.
//!
//! Called from:
//! - The common builtin operand preparation entry point, including callable wrappers.
//!
//! Key details:
//! - Neutral contract metadata selects the policy; selected signature names locate mode slots.
//! - Guards retain source effects and operand owners, and emit target-independent EIR.

use super::*;

/// Rejects automatic PHP comparisons before entering a builtin implementation.
pub(super) fn validate(
    ctx: &mut LoweringContext<'_, '_>, definition: &crate::builtins::registry::BuiltinDef,
    signature: Option<&FunctionSig>, operands: &[crate::ir::ValueId], span: Span,
) {
    if !crate::semantics::comparisons::active(ctx.semantic_profile, ctx.builtin_source_mode()) { return; }
    let Some(policy) = definition.spec.comparison_policy() else { return; };
    if !policy.requires_argument_check() { return; }
    let pins = pin_in_flight_owners(ctx, operands, span);
    let accepted = match policy {
        elephc_builtin_contract::ComparisonPolicy::Identity { parameter } => {
            let parameters = signature.map_or(&definition.params, |signature| &signature.params);
            let mode = parameters.iter().position(|(name, _)| name == parameter)
                .and_then(|index| operands.get(index)).copied();
            if let Some(value) = mode {
                let value = lowered_value_from_id(ctx, value);
                ctx.truthy(value, Some(span))
            } else { emit_bool_literal(ctx, false, Some(span)) }
        }
        elephc_builtin_contract::ComparisonPolicy::ComparatorRequired { .. } =>
            emit_bool_literal(ctx, false, Some(span)),
        elephc_builtin_contract::ComparisonPolicy::Comparator { .. } =>
            unreachable!("callback results are checked at their invocation boundary"),
    };
    crate::ir_lower::semantic_guards::require(ctx, accepted, Op::ThrowTypeError,
        &policy.message(definition.name), Some(span));
    unpin_in_flight_owners(ctx, pins, span);
}
