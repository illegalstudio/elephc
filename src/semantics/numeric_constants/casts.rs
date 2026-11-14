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

/// Shares GMP-specific original-source proofs without replacing the native string grammar.
pub(super) fn gmp_builtin(expr: &crate::parser::ast::Expr, name: &str,
    args: &[crate::parser::ast::Expr], class: Option<&str>, definitions: &Definitions, evidence: &Evidence)
    -> Result<Option<Number>, crate::errors::CompileError> {
    let Some(normalized) = planned_builtin_arguments(name, args, expr.span) else { return Ok(None); };
    let mut call = Evidence { profile: evidence.profile, active: evidence.active.clone(), strict_types: evidence.strict_types, ..Default::default() };
    let Some(value) = normalized.first() else { return Ok(None); };
    let Some(value) = evaluate(value, true, class, definitions, &mut call)? else { return Ok(None); };
    elephc_semantics::gmp_integer_cast::prove_gmp_integer_cast(scalar_input(value, &call), evidence.strict_types)
        .map(|value| value.map(Number::Int)).map_err(|failure| crate::errors::CompileError::new(expr.span, failure.message()))
}

/// Validates original settype targets and numeric values before folding can erase their call.
pub(super) fn settype_builtin(expr: &crate::parser::ast::Expr, name: &str,
    args: &[crate::parser::ast::Expr], class: Option<&str>, definitions: &Definitions, evidence: &Evidence)
    -> Result<(), crate::errors::CompileError> {
    let Some(normalized) = planned_builtin_arguments(name, args, expr.span) else { return Ok(()); };
    let [value, target] = normalized.as_slice() else { return Ok(()); };
    let mut call = Evidence { profile: evidence.profile, active: evidence.active.clone(), strict_types: evidence.strict_types, ..Default::default() };
    let Some(target) = evaluate(target, true, class, definitions, &mut call)? else { return Ok(()); };
    let NumericCastInput::String(target) = scalar_input(target, &call) else { return Ok(()); };
    let target = elephc_semantics::settype_target::SettypeTarget::parse(target)
        .map_err(|failure| crate::errors::CompileError::new(expr.span, failure.message()))?;
    let Some(target) = target.numeric_target() else { return Ok(()); };
    let Some(value) = evaluate(value, true, class, definitions, &mut call)? else { return Ok(()); };
    checked_numeric_cast(target, scalar_input(value, &call))
        .map_err(|failure| crate::errors::CompileError::new(expr.span, failure.message()))?;
    Ok(())
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

    /// Invalid settype targets remain errors through aliases, argument planning and removed branches.
    #[test]
    fn strict_settype_source_targets_precede_folding() {
        let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Casts, true);
        for source in ["<?php if (false) { settype($value, 'unset'); }",
            "<?php const T = 'resource'; settype(type: T, var: $value);",
            "<?php const T = 'wrong'; settype(...['type' => T, 'var' => $value]);",
            "<?php function f($value = settype($other, 'int ')) {}",
            "<?php eval('settype($value, \"unset\");');"] {
            let program = crate::parser::parse(&crate::lexer::tokenize(source).unwrap()).unwrap();
            let error = crate::semantics::validate_source(&program, profile).unwrap_err();
            assert!(error.message.contains("--strict-casts"), "{source} {error:?}");
            assert!(crate::semantics::validate_source(&program, SemanticProfile::PERMISSIVE).is_ok());
        }
    }

    /// Scalar alias proofs do not misclassify valid nonnumeric, unknown or trusted targets.
    #[test]
    fn strict_settype_source_targets_preserve_valid_and_internal_controls() {
        let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Casts, true);
        for source in ["<?php settype($value, 'INTEGER'); settype($value, 'BOOLEAN');",
            "<?php settype($value, 'array'); settype($value, 'object'); settype($value, 'null');",
            "<?php settype($value, $target);"] {
            let program = crate::parser::parse(&crate::lexer::tokenize(source).unwrap()).unwrap();
            assert!(crate::semantics::validate_source(&program, profile).is_ok(), "{source}");
        }
        let program = crate::parser::parse_with_mode(&crate::lexer::tokenize(
            "<?php settype($value, 'unset');").unwrap(), SourceMode::Internal).unwrap();
        assert!(crate::semantics::validate_source(&program, profile).is_ok());
    }

    /// GMP metadata proves original decimal magnitudes and weak floats in all source-bearing contexts.
    #[test]
    fn strict_gmp_cast_source_proofs_precede_folding() {
        let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Casts, true);
        for source in ["<?php if (false) { gmp_intval('9223372036854775808'); }",
            "<?php const V = '-9223372036854775809'; echo gmp_intval(num: V);",
            "<?php const V = '9223372036854775808'; echo gmp_intval(...['num' => V]);",
            "<?php function f($v = gmp_intval(1e999)) {}",
            "<?php class C { const V = gmp_intval('9223372036854775808'); }",
            "<?php echo gmp_intval([]);", "<?php eval('gmp_intval(9223372036854775808.0);');"] {
            for mode in [SourceMode::Php, SourceMode::Lfc] {
                let program = crate::parser::parse_with_mode(&crate::lexer::tokenize(source).unwrap(), mode).unwrap();
                let error = crate::semantics::validate_source(&program, profile).unwrap_err();
                assert!(error.message.contains("--strict-casts"), "{source} {error:?}");
                assert!(crate::semantics::validate_source(&program, SemanticProfile::PERMISSIVE).is_ok());
            }
        }
    }

    /// Native spellings remain unknown instead of acquiring intval grammar or incorrect numeric evidence.
    #[test]
    fn strict_gmp_cast_source_proofs_preserve_native_and_internal_domains() {
        let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Casts, true);
        for source in ["<?php echo gmp_intval('9223372036854775807'); echo gmp_intval('-9223372036854775808');",
            "<?php echo gmp_intval('0xff'); echo gmp_intval('0o17'); echo gmp_intval('077');",
            "<?php echo gmp_intval('1 2'); echo gmp_intval($unknown);",
            "<?php declare(strict_types=1); echo gmp_intval(1e999);",
            "<?php function gmp_intval($v) { return 0; } echo gmp_intval('9223372036854775808');"] {
            let program = crate::parser::parse(&crate::lexer::tokenize(source).unwrap()).unwrap();
            assert!(crate::semantics::validate_source(&program, profile).is_ok(), "{source}");
        }
        let program = crate::parser::parse_with_mode(&crate::lexer::tokenize(
            "<?php const V = gmp_intval('9223372036854775808');").unwrap(), SourceMode::Internal).unwrap();
        assert!(crate::semantics::validate_source(&program, profile).is_ok());
    }

    /// A proven checked GMP result can participate in independent overflow validation.
    #[test]
    fn strict_gmp_cast_source_proofs_retain_composed_integer_results() {
        let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Casts, true).with(StrictFeature::Overflow, true);
        let program = crate::parser::parse(&crate::lexer::tokenize(
            "<?php echo gmp_intval('9223372036854775807') + 1;").unwrap()).unwrap();
        let error = crate::semantics::validate_source(&program, profile).unwrap_err();
        assert!(error.message.contains("--strict-overflow"), "{error:?}");
        assert!(crate::semantics::validate_source(&program, profile.with(StrictFeature::Casts, false)).is_ok());
        let program = crate::parser::parse(&crate::lexer::tokenize(
            "<?php echo gmp_intval('077') ** 10;").unwrap()).unwrap();
        assert!(crate::semantics::validate_source(&program, profile).is_ok());
    }

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
