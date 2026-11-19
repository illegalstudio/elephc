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
    !type_accepts_null(&crate::eval_ir::generic_type(ty))
}

/// Reads compound source contracts without projecting internal content schemas to a PHP kind.
fn type_accepts_null(ty: &elephc_generics::Type) -> bool {
    use elephc_generics::Type;
    if crate::eval_ir::array_contents_schema(ty).is_some() { return true; }
    match ty {
        Type::Atom(name) => matches!(name.as_str(), "mixed" | "null" | "void"),
        Type::Nullable(_) => true,
        Type::Union(members) => members.iter().any(type_accepts_null),
        Type::Intersection(members) => members.iter().all(type_accepts_null),
        _ => false,
    }
}
