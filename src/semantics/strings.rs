//! Purpose:
//! Shares source string contracts between syntax validation and typed consumers.
//!
//! Called from:
//! - String concatenation and output checks before backend storage adaptation.
//!
//! Key details:
//! - Concatenation accepts original strings; output is an explicit formatting context.
//! - Partial union membership stays a runtime obligation, independently of Coercions.

use crate::{errors::CompileError, source::SourceMode, span::Span, types::PhpType};
use elephc_semantics::{SemanticProfile, StrictFeature, conversions::{
    plan_conversion, ConversionPlan, ConversionSite, ValueKind,
}};

pub(crate) use elephc_semantics::conversions::{
    STRING_CONCAT_MESSAGE as CONCAT_MESSAGE, STRING_FORMAT_MESSAGE as FORMAT_MESSAGE,
};

/// Preserves formatting when a prepared expression must be serialized back to PHP source.
pub(crate) fn formatting_source(expression: &str) -> String {
    if expression.starts_with('$') { return format!("\"{{{expression}}}\""); }
    // PHP braced interpolation requires a variable prefix. Other prepared expressions
    // are evaluated once as a closure argument, without introducing a caller-visible local.
    format!("(static function ($__elephc_format_value) {{ return \"{{$__elephc_format_value}}\"; }})({expression})")
}

/// Applies user string guarantees without changing trusted implementation expressions.
pub(crate) fn active(profile: SemanticProfile, source: SourceMode) -> bool {
    source != SourceMode::Internal && profile.contains(StrictFeature::Strings)
}

/// Projects logical alternatives without confusing a boxed representation with PHP identity.
pub(crate) fn concatenation(profile: SemanticProfile, ty: &PhpType) -> ConversionPlan {
    if let PhpType::Union(members) = ty {
        let plans: Vec<_> = members.iter().map(|member| concatenation(profile, member)).collect();
        if plans.iter().all(|plan| *plan == ConversionPlan::Identity) { return ConversionPlan::Identity; }
        if plans.iter().all(|plan| matches!(plan, ConversionPlan::Reject(_))) {
            return ConversionPlan::Reject(StrictFeature::Strings);
        }
        return plan_conversion(profile, ConversionSite::Concatenation, ValueKind::Dynamic, ValueKind::String);
    }
    if *ty == PhpType::Never { return ConversionPlan::Identity; }
    plan_conversion(profile, ConversionSite::Concatenation,
        super::numeric::value_kind(ty), ValueKind::String)
}

/// Rejects statically incompatible original operands while retaining valid union alternatives.
pub(crate) fn validate_concatenation(profile: SemanticProfile, source: SourceMode,
    ty: &PhpType, span: Span) -> Result<(), CompileError> {
    if active(profile, source) && matches!(concatenation(profile, ty), ConversionPlan::Reject(_)) {
        return Err(CompileError::new(span, CONCAT_MESSAGE));
    }
    Ok(())
}

/// Identifies an array in every logical alternative, before an implicit Array string can hide it.
fn always_array(ty: &PhpType) -> bool {
    match ty {
        PhpType::Array(_) | PhpType::AssocArray { .. } => true,
        PhpType::Union(members) => !members.is_empty() && members.iter().all(always_array),
        PhpType::Narrow(_) => always_array(&ty.storage_value_type()),
        _ => false,
    }
}

/// Keeps scalar and Stringable output available while refusing known implicit array formatting.
pub(crate) fn validate_formatting(profile: SemanticProfile, source: SourceMode,
    ty: &PhpType, span: Span) -> Result<(), CompileError> {
    if active(profile, source) && always_array(ty) {
        return Err(CompileError::new(span, FORMAT_MESSAGE));
    }
    Ok(())
}
