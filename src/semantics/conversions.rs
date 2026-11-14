//! Purpose:
//! Projects full checker declarations into shared implicit value-conversion plans.
//!
//! Called from:
//! - Parameter checking and EIR value boundaries before storage adaptation.
//!
//! Key details:
//! - Scalar unions retain exact PHP kinds; nominal assignability stays with the checker.
//! - Trusted implementation operations keep their existing conversion contracts.

use crate::{source::SourceMode, types::PhpType};
use elephc_semantics::{SemanticProfile, conversions::{AllowedKinds, ConversionPlan, ValueKind}};

/// Extracts a scalar declaration without flattening its PHP identity into physical storage.
pub(crate) fn scalar_contract(ty: &PhpType) -> Option<AllowedKinds> {
    Some(match ty {
        PhpType::Int => AllowedKinds::one(ValueKind::Integer),
        PhpType::Float => AllowedKinds::one(ValueKind::Float),
        PhpType::Bool | PhpType::False => AllowedKinds::one(ValueKind::Boolean),
        PhpType::Str => AllowedKinds::one(ValueKind::String),
        PhpType::Void => AllowedKinds::one(ValueKind::Null),
        PhpType::Union(members) => members.iter().try_fold(AllowedKinds::EMPTY,
            |allowed, member| Some(allowed.union(scalar_contract(member)?)))?,
        _ => return None,
    })
}

/// Distinguishes the literal false declaration from a contract accepting every boolean.
pub(crate) fn only_false(ty: &PhpType) -> bool {
    match ty {
        PhpType::False => true,
        PhpType::Union(members) => members.iter().any(only_false) && !members.contains(&PhpType::Bool),
        _ => false,
    }
}

/// Resolves one implicit scalar boundary independently of PHP's strict_types directive.
pub(crate) fn implicit(profile: SemanticProfile, source: SourceMode, expected: &PhpType,
    actual: &PhpType) -> ConversionPlan {
    if source == SourceMode::Internal { return ConversionPlan::ExistingBehavior; }
    let Some(allowed) = scalar_contract(expected) else { return ConversionPlan::ExistingBehavior; };
    if let PhpType::Union(members) = actual {
        let plans: Vec<_> = members.iter().map(|member| implicit(profile, source, expected, member)).collect();
        if plans.iter().all(|plan| *plan == ConversionPlan::Identity) { return ConversionPlan::Identity; }
        if plans.iter().all(|plan| matches!(plan, ConversionPlan::Reject(_))) {
            return ConversionPlan::Reject(elephc_semantics::StrictFeature::Coercions);
        }
        return elephc_semantics::conversions::plan_implicit_kinds(profile, ValueKind::Dynamic, allowed);
    }
    if *actual == PhpType::Never { return ConversionPlan::Identity; }
    if only_false(expected) && *actual == PhpType::Bool {
        return elephc_semantics::conversions::plan_implicit_kinds(profile, ValueKind::Dynamic, allowed);
    }
    elephc_semantics::conversions::plan_implicit_kinds(profile, super::numeric::value_kind(actual), allowed)
}
