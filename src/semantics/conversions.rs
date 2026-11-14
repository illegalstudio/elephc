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

/// Projects only scalar source annotations whose meaning needs no class or generic context.
pub(crate) fn scalar_hint(hint: &crate::parser::ast::TypeExpr) -> Option<PhpType> {
    use crate::parser::ast::TypeExpr;
    Some(match hint {
        TypeExpr::Int => PhpType::Int, TypeExpr::Float => PhpType::Float,
        TypeExpr::Bool => PhpType::Bool, TypeExpr::False => PhpType::False,
        TypeExpr::Str => PhpType::Str, TypeExpr::Void => PhpType::Void,
        TypeExpr::Nullable(inner) => PhpType::Union(vec![scalar_hint(inner)?, PhpType::Void]),
        TypeExpr::Union(members) => PhpType::Union(members.iter().map(scalar_hint).collect::<Option<Vec<_>>>()?),
        TypeExpr::Named(name) => match name.as_str().to_ascii_lowercase().as_str() {
            "int" => PhpType::Int, "float" => PhpType::Float, "bool" => PhpType::Bool,
            "false" => PhpType::False, "string" => PhpType::Str, "null" | "void" => PhpType::Void,
            _ => return None,
        },
        _ => return None,
    })
}

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

/// Projects scalar or nominal declarations while leaving nominal membership to typed adapters.
pub(crate) fn implicit_contract(ty: &PhpType) -> Option<AllowedKinds> {
    match ty {
        PhpType::Object(_) => Some(AllowedKinds::one(ValueKind::NonScalar)),
        PhpType::Union(members) => members.iter().try_fold(AllowedKinds::EMPTY,
            |allowed, member| Some(allowed.union(implicit_contract(member)?))),
        _ => scalar_contract(ty),
    }
}

/// Retains nominal syntax for conservative effects without resolving it to an ABI representation.
pub(crate) fn implicit_hint(hint: &crate::parser::ast::TypeExpr) -> Option<PhpType> {
    let ty = crate::types::constant_binding::declared_type(hint);
    implicit_contract(&ty).map(|_| ty)
}

/// Collects nominal alternatives separately from the neutral scalar kind projection.
pub(crate) fn nominal_members(ty: &PhpType) -> Vec<&str> {
    match ty {
        PhpType::Object(name) => vec![name],
        PhpType::Union(members) => members.iter().flat_map(nominal_members).collect(),
        _ => Vec::new(),
    }
}

/// Distinguishes the literal false declaration from a contract accepting every boolean.
pub(crate) fn only_false(ty: &PhpType) -> bool {
    match ty {
        PhpType::False => true,
        PhpType::Union(members) => members.iter().any(only_false) && !members.contains(&PhpType::Bool),
        _ => false,
    }
}

/// Preserves original scalar alternatives when an inferred result has no coercive declaration.
pub(crate) fn inferred_scalar_join(profile: SemanticProfile, source: SourceMode,
    left: &PhpType, right: &PhpType) -> Option<PhpType> {
    if source == SourceMode::Internal || !profile.contains(elephc_semantics::StrictFeature::Coercions)
        || scalar_contract(left).is_none() || scalar_contract(right).is_none() { return None; }
    Some(crate::types::checker::normalize_union_members(vec![left.clone(), right.clone()]))
}

/// Resolves one implicit scalar boundary independently of PHP's strict_types directive.
pub(crate) fn implicit(profile: SemanticProfile, source: SourceMode, expected: &PhpType,
    actual: &PhpType) -> ConversionPlan {
    if source == SourceMode::Internal { return ConversionPlan::ExistingBehavior; }
    let Some(allowed) = implicit_contract(expected) else { return ConversionPlan::ExistingBehavior; };
    if let PhpType::Union(members) = actual {
        let plans: Vec<_> = members.iter().map(|member| implicit(profile, source, expected, member)).collect();
        if plans.iter().all(|plan| *plan == ConversionPlan::Identity) { return ConversionPlan::Identity; }
        if plans.iter().all(|plan| matches!(plan, ConversionPlan::Reject(_))) {
            return ConversionPlan::Reject(elephc_semantics::StrictFeature::Coercions);
        }
        return elephc_semantics::conversions::plan_implicit_kinds(profile, ValueKind::Dynamic, allowed);
    }
    if *actual == PhpType::Never { return ConversionPlan::Identity; }
    if !nominal_members(expected).is_empty() && super::numeric::value_kind(actual) == ValueKind::NonScalar {
        return elephc_semantics::conversions::plan_implicit_kinds(profile, ValueKind::Dynamic, allowed);
    }
    if only_false(expected) && *actual == PhpType::Bool {
        return elephc_semantics::conversions::plan_implicit_kinds(profile, ValueKind::Dynamic, allowed);
    }
    elephc_semantics::conversions::plan_implicit_kinds(profile, super::numeric::value_kind(actual), allowed)
}
