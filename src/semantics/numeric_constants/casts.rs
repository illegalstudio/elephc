//! Purpose:
//! Preserves checked explicit cast evidence before destructive constant folding.
//!
//! Called from:
//! - The original-source numeric validator, including aliases and declaration defaults.
//!
//! Key details:
//! - Decimal strings carry an exact integer conversion beside their floating arithmetic value.

use super::Number;
use crate::parser::ast::CastType;
use elephc_semantics::{numeric_cast::{checked_numeric_cast, numeric_cast_failure,
    NumericCastFailure, NumericCastInput, NumericCastTarget}, numeric_unary::NumericValue};

/// Projects proven original scalar kinds into checked cast results without guessing unknowns.
pub(super) fn evaluate_cast(target: &CastType, input: Option<Number>) -> Result<Option<Number>, NumericCastFailure> {
    let target = match target { CastType::Int => NumericCastTarget::Int, CastType::Float => NumericCastTarget::Float,
        _ => return Ok(None) };
    let Some(input) = input else { return Ok(None); };
    let value = match input {
        Number::StringInt { fully_numeric: false, .. } | Number::StringFloat { fully_numeric: false, .. }
        | Number::NonNumericString { .. } => return Err(NumericCastFailure::InvalidString),
        Number::StringInt { value, .. } => NumericCastInput::Int(value),
        Number::StringFloat { checked_integer, .. } if target == NumericCastTarget::Int => {
            return checked_integer.map(|value| Some(Number::Int(value))).map_err(numeric_cast_failure);
        }
        Number::StringFloat { value, .. } | Number::Float(value) => NumericCastInput::Float(value),
        Number::Int(value) => NumericCastInput::Int(value), Number::Bool(value) => NumericCastInput::Bool(value),
        Number::Null => NumericCastInput::Null, Number::Array => NumericCastInput::Unsupported,
    };
    checked_numeric_cast(target, value).map(|value| Some(match value {
        NumericValue::Int(value) => Number::Int(value), NumericValue::Float(value) => Number::Float(value),
    }))
}

#[cfg(test)]
mod tests {
    use super::super::*;

    /// Cast syntax survives source auditing in dead branches, defaults, aliases and literal eval.
    #[test]
    fn strict_numeric_cast_source_domains_are_checked_before_folding() {
        let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Casts, true);
        for source in ["<?php if (false) { echo (int)'12tail'; }",
            "<?php function f($v = (float)'junk') {}", "<?php const V = '9223372036854775808'; echo (int)V;",
            "<?php class C { const V = '1x'; const W = (float)self::V; }",
            "<?php echo (int)1e999;", "<?php echo (int)[];", "<?php eval('echo (int)\"1x\";');"] {
            for mode in [SourceMode::Php, SourceMode::Lfc] {
                let program = crate::parser::parse_with_mode(&crate::lexer::tokenize(source).unwrap(), mode).unwrap();
                let error = crate::semantics::validation::validate_source(&program, profile).unwrap_err();
                assert!(error.message.contains("--strict-casts"), "{source} {error:?}");
                assert!(crate::semantics::validation::validate_source(&program, SemanticProfile::PERMISSIVE).is_ok());
            }
        }
    }

    /// Numeric casts remain explicit conversions with independent arithmetic and coercion policies.
    #[test]
    fn strict_numeric_cast_source_accepts_exact_results_unknowns_and_trusted_producers() {
        let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Casts, true)
            .with(StrictFeature::Coercions, true).with(StrictFeature::Arithmetic, true);
        for source in ["<?php echo (int)'9223372036854775807.9'; echo (int)'-9223372036854775808.9';",
            "<?php echo (int)'92233720368547758070e-1'; echo (float)'1e999';",
            "<?php echo (int)true; echo (float)null; echo (int)$unknown;",
            "<?php const V = '42'; function f(int $v = (int)V) {}"] {
            let program = crate::parser::parse(&crate::lexer::tokenize(source).unwrap()).unwrap();
            assert!(crate::semantics::validation::validate_source(&program, profile).is_ok(), "{source}");
        }
        let program = crate::parser::parse_with_mode(&crate::lexer::tokenize("<?php echo (int)'12tail';").unwrap(), SourceMode::Internal).unwrap();
        assert!(crate::semantics::validation::validate_source(&program, profile).is_ok());
    }
}
