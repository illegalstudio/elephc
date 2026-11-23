//! Purpose:
//! Projects original array-key kinds into the shared exact-key domain.
//!
//! Called from:
//! - Source validation, array inference and array lvalue checking.
//!
//! Key details:
//! - Dynamic alternatives need a guard; strings and ArrayAccess receivers keep separate rules.

use crate::{errors::CompileError, span::Span, types::PhpType};
use elephc_semantics::{SemanticProfile, StrictFeature,
    conversions::{plan_conversion, ConversionPlan, ConversionSite, ValueKind}};

mod source_writes;
pub(crate) use source_writes::{validate_source_writes, SourceWriteScope};

pub(crate) use elephc_semantics::array_keys::EXACT_KEY_MESSAGE;

/// Plans original key membership without enabling coercion, indexing or array element policies.
pub(crate) fn plan(profile: SemanticProfile, ty: &PhpType) -> ConversionPlan {
    if let PhpType::Union(members) = ty {
        let plans: Vec<_> = members.iter().map(|member| plan(profile, member)).collect();
        if plans.iter().all(|plan| *plan == ConversionPlan::Identity) { return ConversionPlan::Identity; }
        if plans.iter().all(|plan| matches!(plan, ConversionPlan::Reject(_))) {
            return ConversionPlan::Reject(StrictFeature::ArrayKeys);
        }
        return plan_conversion(profile, ConversionSite::ArrayKey, ValueKind::Dynamic, ValueKind::Dynamic);
    }
    if *ty == PhpType::Never { return ConversionPlan::Identity; }
    plan_conversion(profile, ConversionSite::ArrayKey, super::numeric::value_kind(ty), ValueKind::Dynamic)
}

/// Refuses a proven invalid original key while leaving unknown kinds to runtime validation.
pub(crate) fn validate(profile: SemanticProfile, ty: &PhpType, span: Span) -> Result<(), CompileError> {
    if matches!(plan(profile, ty), ConversionPlan::Reject(_)) {
        return Err(CompileError::new(span, EXACT_KEY_MESSAGE));
    }
    Ok(())
}

/// Restricts only receivers proven to be PHP arrays, including array-only unions.
pub(crate) fn validate_receiver(profile: SemanticProfile, receiver: &PhpType,
    key: &PhpType, span: Span) -> Result<(), CompileError> {
    if definite_array(receiver, false) { validate(profile, key, span)?; }
    Ok(())
}

/// Includes false/null parents that a write materializes as arrays under existing PHP rules.
pub(crate) fn validate_write_receiver(profile: SemanticProfile, receiver: &PhpType,
    key: &PhpType, span: Span) -> Result<(), CompileError> {
    if definite_array(receiver, true) { validate(profile, key, span)?; }
    Ok(())
}

/// Requires every alternative to identify an array place without narrowing a dynamic receiver.
fn definite_array(ty: &PhpType, for_write: bool) -> bool {
    match ty {
        PhpType::Array(_) | PhpType::AssocArray { .. } => true,
        PhpType::Void | PhpType::False => for_write,
        PhpType::Union(members) => !members.is_empty() && members.iter().all(|ty| definite_array(ty, for_write)),
        _ => false,
    }
}

#[cfg(test)]
mod tests;
