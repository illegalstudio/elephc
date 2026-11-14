//! Purpose:
//! Preserves checked explicit cast evidence before destructive constant folding.
//!
//! Called from:
//! - The original-source numeric validator, including aliases and declaration defaults.
//!
//! Key details:
//! - Original string bytes stay in the source tree's evidence pool beside numeric classification.

use super::{Number, Evidence, Definitions, evaluate, planned_builtin_arguments};
use crate::parser::ast::CastType;
use elephc_semantics::{numeric_cast::{checked_numeric_cast, checked_numeric_builtin_cast,
    NumericCastFailure, NumericCastInput, NumericCastTarget}, numeric_unary::NumericValue};

/// Projects proven original scalar kinds into checked cast results without guessing unknowns.
pub(super) fn evaluate_cast(target: &CastType, input: Option<Number>, evidence: &Evidence) -> Result<Option<Number>, NumericCastFailure> {
    let target = match target { CastType::Int => NumericCastTarget::Int, CastType::Float => NumericCastTarget::Float,
        _ => return Ok(None) };
    let Some(input) = input else { return Ok(None); };
    checked_numeric_cast(target, scalar_input(input, evidence)).map(|value| Some(number(value)))
}

/// Retains original bytes when a numeric classification alone cannot express explicit radix parsing.
fn scalar_input(input: Number, evidence: &Evidence) -> NumericCastInput<'_> {
    match input {
        Number::StringInt { text, .. } | Number::StringFloat { text, .. } | Number::NonNumericString { text, .. } =>
            NumericCastInput::String(&evidence.strings[text]),
        Number::Float(value) => NumericCastInput::Float(value),
        Number::Int(value) => NumericCastInput::Int(value), Number::Bool(value) => NumericCastInput::Bool(value),
        Number::Null => NumericCastInput::Null, Number::Array => NumericCastInput::Unsupported,
    }
}

/// Adapts exact shared numeric results without projecting their kind through storage representation.
fn number(value: NumericValue) -> Number {
    match value {
        NumericValue::Int(value) => Number::Int(value), NumericValue::Float(value) => Number::Float(value),
    }
}

/// Proves registered numeric cast builtins through the same planner used by ordinary call binding.
pub(super) fn builtin(expr: &crate::parser::ast::Expr, name: &str, target: NumericCastTarget,
    args: &[crate::parser::ast::Expr], class: Option<&str>, definitions: &Definitions, evidence: &Evidence)
    -> Result<Option<Number>, crate::errors::CompileError> {
    let Some(normalized) = planned_builtin_arguments(name, args, expr.span) else { return Ok(None); };
    let mut call = Evidence { profile: evidence.profile, active: evidence.active.clone(), strict_types: evidence.strict_types, ..Default::default() };
    let Some(value) = normalized.first() else { return Ok(None); };
    let value = evaluate(value, true, class, definitions, &mut call)?;
    let base = match normalized.get(1) {
        Some(base) => match evaluate(base, true, class, definitions, &mut call)? { Some(Number::Int(value)) => Some(value), _ => None },
        None => Some(10),
    };
    let Some(value) = value else { return Ok(None); };
    checked_numeric_builtin_cast(target, scalar_input(value, &call), base).map(|value| value.map(number))
        .map_err(|failure| crate::errors::CompileError::new(expr.span, failure.message()))
}

#[cfg(test)]
mod tests {
    use super::super::*;

    /// Builtin metadata shares source proofs across original aliases, radix strings and removed branches.
    #[test]
    fn strict_builtin_casts_source_metadata_checks_original_values_before_folding() {
        let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Casts, true);
        for source in ["<?php if (false) { intval('12tail'); }", "<?php function f($v = floatval('junk')) {}",
            "<?php const V = '8000000000000000'; echo intval(base: 16, value: V);",
            "<?php const V = 'fftail'; echo intval(...['base' => 16, 'value' => V]);",
            "<?php class C { const V = 'junk'; const W = floatval(self::V); }",
            "<?php echo intval('ff', 1);", "<?php echo floatval([]);",
            "<?php eval('echo intval(\"junk\");');"] {
            let program = crate::parser::parse(&crate::lexer::tokenize(source).unwrap()).unwrap();
            let error = crate::semantics::validate_source(&program, profile).unwrap_err();
            assert!(error.message.contains("--strict-casts"), "{source} {error:?}");
            assert!(crate::semantics::validate_source(&program, SemanticProfile::PERMISSIVE).is_ok());
        }
    }

    /// Exact strings, ignored bases, uncertain values and trusted producers keep their own contracts.
    #[test]
    fn strict_builtin_casts_source_metadata_preserves_exact_and_uncertain_controls() {
        let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Casts, true);
        for source in ["<?php const V = 'ff'; echo intval(V, 16);", "<?php echo intval('9223372036854775807.9');",
            "<?php echo intval(7.9, 1); echo floatval('1e999');", "<?php echo intval($unknown, $base);",
            "<?php const V = 'ff'; echo intval(V, $base);", "<?php const V = 'ff'; const B = 16; echo intval(V, B);"] {
            let program = crate::parser::parse(&crate::lexer::tokenize(source).unwrap()).unwrap();
            assert!(crate::semantics::validate_source(&program, profile).is_ok(), "{source}");
        }
        let trusted = crate::parser::parse_with_mode(&crate::lexer::tokenize("<?php const V = intval('12tail');").unwrap(), SourceMode::Internal).unwrap();
        assert!(crate::semantics::validate_source(&trusted, profile).is_ok());
    }

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
