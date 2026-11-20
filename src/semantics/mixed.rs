//! Purpose:
//! Validates kind-dependent consumers against every logical source alternative.
//!
//! Called from:
//! - Flow-sensitive operator, receiver and declared-value checking.
//!
//! Key details:
//! - Physical boxing and nested mixed array elements are not dynamic source alternatives.
//! - Full class and callable compatibility stay with the existing checker adapters.

use crate::{errors::CompileError, source::SourceMode, span::Span, types::PhpType};
use elephc_semantics::{SemanticProfile, StrictFeature};

/// Requires an existing flow proof for dynamic alternatives before a restricted consumer.
pub(crate) fn validate_use(profile: SemanticProfile, source: SourceMode, actual: &PhpType,
    span: Span, context: &str, mut supported: impl FnMut(&PhpType) -> bool,
) -> Result<(), CompileError> {
    if source == SourceMode::Internal || !profile.contains(StrictFeature::Mixed)
        || !dynamic_alternatives(actual) || every_alternative(actual, &mut supported) {
        return Ok(());
    }
    Err(CompileError::semantic_violation(span, StrictFeature::Mixed, &format!(
        "--strict-mixed: narrow {actual} before {context}; every alternative must support the operation")))
}

/// Examines the logical outer view, leaving mixed container elements for their own consumers.
fn dynamic_alternatives(ty: &PhpType) -> bool {
    matches!(ty, PhpType::Mixed | PhpType::Union(_) | PhpType::TaggedScalar | PhpType::DeclaredCallable)
}

/// Applies the same resolved predicate to all alternatives without flattening class information.
pub(crate) fn every_alternative(ty: &PhpType, supported: &mut impl FnMut(&PhpType) -> bool) -> bool {
    match ty {
        PhpType::Never => true,
        PhpType::Mixed | PhpType::TaggedScalar => false,
        PhpType::Union(members) => members.iter().all(|member| every_alternative(member, supported)),
        _ => supported(ty),
    }
}

/// Preserves closed numeric alternatives instead of inventing an opaque result after arithmetic.
pub(crate) fn numeric_result(profile: SemanticProfile, source: SourceMode) -> PhpType {
    if source != SourceMode::Internal && profile.contains(StrictFeature::Mixed) {
        PhpType::Union(vec![PhpType::Int, PhpType::Float])
    } else { PhpType::Mixed }
}

/// Reuses object-kind proofs while leaving member existence and visibility with the checker.
pub(crate) fn member_receiver(profile: SemanticProfile, source: SourceMode, actual: &PhpType,
    probe: bool, nullsafe: bool, span: Span) -> Result<(), CompileError> {
    if probe { return Ok(()); }
    validate_use(profile, source, actual, span, "member access", |member|
        matches!(member, PhpType::Object(_) | PhpType::Callable | PhpType::Pointer(_) | PhpType::Packed(_))
            || nullsafe && *member == PhpType::Void)
}

/// Retains the integer result proven by checked integer overflow without changing other numeric joins.
pub(crate) fn integer_result(profile: SemanticProfile, source: SourceMode) -> PhpType {
    if source != SourceMode::Internal && profile.contains(StrictFeature::Mixed)
        && profile.contains(StrictFeature::Overflow) { PhpType::Int }
    else { numeric_result(profile, source) }
}

/// Negative integer powers remain floating-point alternatives unless their own signature forbids them.
pub(crate) fn power_result(profile: SemanticProfile, source: SourceMode) -> PhpType {
    if profile.contains(StrictFeature::Arithmetic) { integer_result(profile, source) }
    else { numeric_result(profile, source) }
}

/// Recognizes PHP array-key source kinds before the independently selected normalization policy.
pub(crate) fn array_key_kind(ty: &PhpType) -> bool {
    matches!(ty, PhpType::Int | PhpType::Float | PhpType::Str | PhpType::Bool | PhpType::False | PhpType::Void)
}

/// Finds receivers whose offset uses PHP's scalar-key protocol rather than an object-defined one.
pub(crate) fn uses_php_offset_key(ty: &PhpType) -> bool {
    match ty {
        PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Str | PhpType::Buffer(_)
            | PhpType::Mixed | PhpType::TaggedScalar => true,
        PhpType::Union(members) => members.iter().any(uses_php_offset_key),
        _ => false,
    }
}
