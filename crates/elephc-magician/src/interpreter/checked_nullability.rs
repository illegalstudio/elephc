//! Purpose:
//! Proves borrowed runtime values non-null before ordinary eval consumers use them.
//!
//! Called from:
//! - Dimension receiver reads and other declared non-null value boundaries.
//!
//! Key details:
//! - The caller keeps its existing operand owners through a pending Throwable.
//! - No conversion, retain or native exception jump crosses this guard.

use super::*;
use elephc_semantics::nullability::{NullPlan, NullState, plan_non_null, NON_NULL_MESSAGE};

/// Rejects actual null without imposing any additional scalar or container kind contract.
pub(super) fn require_non_null(value: RuntimeCellHandle, context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let profile = context.semantic_profile();
    if !profile.contains(elephc_semantics::StrictFeature::Nullability) { return Ok(()); }
    let state = if values.is_null(value)? { NullState::Null } else { NullState::NonNull };
    if plan_non_null(profile, state, false) == NullPlan::Reject {
        return eval_throw_type_error(NON_NULL_MESSAGE, context, values);
    }
    Ok(())
}

/// Keeps explicit nullable/mixed declarations open while checking original non-null candidates.
pub(super) fn declared(value: RuntimeCellHandle, ty: &EvalParameterType,
    context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if !non_null_contract(ty) { return Ok(()); }
    require_non_null(value, context, values)
}

/// Classifies the complete declaration before a reference preparation can retarget storage.
pub(super) fn non_null_contract(ty: &EvalParameterType) -> bool {
    if ty.allows_null() || !ty.is_intersection() && ty.variants().iter().any(|variant|
        matches!(variant, EvalParameterTypeVariant::Mixed | EvalParameterTypeVariant::Void)
            || matches!(variant, EvalParameterTypeVariant::Generic(ty)
                if crate::eval_ir::array_contents_schema(ty).is_some())) {
        return false;
    }
    true
}
