//! Purpose:
//! Applies shared builtin comparison metadata to already evaluated eval operands.
//!
//! Called from:
//! - Positional value hooks and the shared named/reference argument binder.
//!
//! Key details:
//! - Existing signature names locate mode roles without matching source arguments again.
//! - The callee retains normal argument coercion; this check only selects the comparison mode.
//! - Handles remain borrowed from the existing call-owner ledger.

use super::*;

/// Requires an explicit identity mode or a source-selected comparator before builtin execution.
pub(super) fn validate(
    specification: &EvalBuiltinSpec, argument: impl Fn(usize) -> Option<RuntimeCellHandle>,
    context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if !context.semantic_profile().contains(elephc_semantics::StrictFeature::Comparisons) { return Ok(()); }
    let Some(policy) = elephc_builtin_contract::lookup(specification.name)
        .and_then(|contract| contract.comparison_policy()) else { return Ok(()); };
    let accepted = match policy {
        elephc_builtin_contract::ComparisonPolicy::Identity { parameter } => {
            let value = specification.param_names.iter().position(|name| *name == parameter)
                .and_then(argument);
            match value { Some(value) => values.truthy(value)?, None => false }
        }
        elephc_builtin_contract::ComparisonPolicy::ComparatorRequired { .. } => false,
        elephc_builtin_contract::ComparisonPolicy::Comparator { .. } => true,
    };
    if accepted { Ok(()) }
    else { eval_throw_type_error(&policy.message(specification.name), context, values) }
}
