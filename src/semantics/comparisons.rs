//! Purpose:
//! Adapts checked source kinds to the shared noncoercive comparison policy.
//!
//! Called from:
//! - Source validation, typed operators and strict switch label checking.
//!
//! Key details:
//! - Unions admit runtime checks only when at least one exact scalar pair is possible.
//! - Trusted implementation expressions retain their existing PHP behavior.

use crate::{errors::CompileError, parser::ast::BinOp, source::SourceMode, span::Span, types::PhpType};
use elephc_semantics::{SemanticProfile, StrictFeature, comparisons::{ordering, OrderingPlan}};

/// Selects only PHP-visible comparison boundaries without depending on CLI state.
pub(crate) fn active(profile: SemanticProfile, source: SourceMode) -> bool {
    source != SourceMode::Internal && profile.contains(StrictFeature::Comparisons)
}

/// Identifies relational and three-way ordering while leaving identity tests unrestricted.
pub(crate) fn is_ordering(operator: &BinOp) -> bool {
    matches!(operator, BinOp::Lt | BinOp::LtEq | BinOp::Gt | BinOp::GtEq | BinOp::Spaceship)
}

/// Recurses through logical unions rather than their boxed physical representation.
pub(crate) fn plan(left: &PhpType, right: &PhpType) -> OrderingPlan {
    if let PhpType::Union(members) = left { return combine(members.iter().map(|left| plan(left, right))); }
    if let PhpType::Union(members) = right { return combine(members.iter().map(|right| plan(left, right))); }
    ordering(super::numeric::value_kind(left), super::numeric::value_kind(right))
}

/// Keeps an uncertain pair guarded unless every possible pair is rejected or exact.
fn combine(plans: impl Iterator<Item = OrderingPlan>) -> OrderingPlan {
    let plans: Vec<_> = plans.collect();
    if plans.iter().all(|plan| *plan == OrderingPlan::Reject) { OrderingPlan::Reject }
    else if plans.iter().all(|plan| *plan == OrderingPlan::Exact) { OrderingPlan::Exact }
    else { OrderingPlan::Guard }
}

/// Reports source-known ordering conversions before normal PHP inference can accept them.
pub(crate) fn validate(profile: SemanticProfile, source: SourceMode, operator: &BinOp,
    left: &PhpType, right: &PhpType, span: Span) -> Result<(), CompileError> {
    if !active(profile, source) { return Ok(()); }
    if matches!(operator, BinOp::Eq | BinOp::NotEq) {
        return Err(CompileError::new(span, elephc_semantics::comparisons::EQUALITY_MESSAGE));
    }
    if is_ordering(operator) && plan(left, right) == OrderingPlan::Reject {
        return Err(CompileError::new(span, elephc_semantics::comparisons::ORDERING_MESSAGE));
    }
    Ok(())
}

/// Reports whether strict identity can share a PHP kind without applying scalar conversion.
pub(crate) fn identity_kinds_overlap(left: &PhpType, right: &PhpType) -> bool {
    identity_kind_mask(left) & identity_kind_mask(right) != 0
}

/// Preserves array layout alternatives and iterable/object overlap without erasing scalar kinds.
fn identity_kind_mask(ty: &PhpType) -> u16 {
    match ty {
        PhpType::Int => 1, PhpType::Float => 2, PhpType::Bool | PhpType::False => 4,
        PhpType::Str => 8, PhpType::Void => 16,
        PhpType::Array(_) | PhpType::AssocArray { .. } => 32,
        PhpType::Object(_) | PhpType::Callable => 64, PhpType::Resource(_) => 128,
        PhpType::Packed(_) | PhpType::Pointer(_) => 256, PhpType::Buffer(_) => 512,
        PhpType::DeclaredCallable => 8 | 32 | 64,
        PhpType::Iterable => 32 | 64, PhpType::Mixed | PhpType::Never => 1023,
        PhpType::TaggedScalar => 1 | 16,
        PhpType::Union(members) => members.iter().fold(0, |mask, member| mask | identity_kind_mask(member)),
        PhpType::Narrow(_) => identity_kind_mask(&ty.storage_value_type()),
    }
}
