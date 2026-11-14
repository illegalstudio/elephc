//! Purpose:
//! Checks proven original declaration values and materializes typed constant result kinds.
//!
//! Called from:
//! - Shared source AST declaration observations and constant alias evaluation.
//!
//! Key details:
//! - Trusted declaration conversions stay local to the producer; consumers see its PHP value kind.
//! - Uncertain binding or conversion rules leave evidence unknown instead of guessing a value.

use super::*;
use elephc_semantics::conversions::{plan_implicit_kinds, ConversionPlan};

/// Rejects only a proven user declaration kind that requires an implicit PHP conversion.
pub(super) fn check(annotation: &TypeExpr, value: &Expr, source: SourceMode, class: Option<&str>,
    definitions: &Definitions, evidence: &mut Evidence) -> Result<(), CompileError> {
    if source == SourceMode::Internal || !evidence.profile.contains(StrictFeature::Coercions) { return Ok(()); }
    let Some(expected) = super::super::conversions::scalar_hint(annotation) else { return Ok(()); };
    let Some(actual) = evaluate(value, true, class, definitions, evidence)? else { return Ok(()); };
    let allowed = super::super::conversions::scalar_contract(&expected).expect("scalar source hint");
    if matches!(plan_implicit_kinds(evidence.profile, actual.value_kind(), allowed), ConversionPlan::Reject(_))
        || super::super::conversions::only_false(&expected) && matches!(actual, Number::Bool(true)) {
        return Err(CompileError::new(value.span, &format!(
            "--strict-coercions: declaration value requires {expected} without implicit conversion, got {:?}",
            actual.value_kind())));
    }
    Ok(())
}

/// Projects a valid typed constant's PHP result rather than its unbound initializer kind.
pub(super) fn apply_contract(value: Option<Number>, annotation: Option<&TypeExpr>) -> Option<Number> {
    let Some(annotation) = annotation else { return value; };
    let Some(expected) = super::super::conversions::scalar_hint(annotation) else { return value; };
    let value = value?;
    if expected.coerces_integer_to_float() {
        if let Number::Int(value) = value { return Some(Number::Float(value as f64)); }
    }
    let allowed = super::super::conversions::scalar_contract(&expected)?;
    (allowed.allows(value.value_kind())
        && !(super::super::conversions::only_false(&expected) && matches!(value, Number::Bool(true))))
        .then_some(value)
}
