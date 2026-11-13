//! Purpose:
//! Tests checked integer-literal parsing in eval independently from native execution.
//!
//! Called from:
//! - Focused Magician parser tests.
//!
//! Key details:
//! - Signed minima share compiler rules; deliberate floats retain IEEE behavior.

use super::support::*;
use super::super::parse_fragment_with_semantics;
use elephc_semantics::{SemanticProfile, StrictFeature};

/// Parses with the checked spelling policy while preserving the ordinary parser wrapper.
fn checked(source: &str) -> Result<EvalProgram, EvalParseError> {
    parse_fragment_with_semantics(source.as_bytes(), SemanticProfile::PERMISSIVE.with(StrictFeature::Overflow, true))
}

/// Eval uses exact signed-minimum values in decimal and prefixed bases.
#[test]
fn strict_integer_literal_eval_parser_keeps_signed_minima_exact() {
    for spelling in ["-9223372036854775808", "-9_223_372_036_854_775_808", "-0x8000000000000000", "-0o1000000000000000000000"] {
        let program = checked(&format!("return {spelling};")).unwrap();
        assert_eq!(program.statements(), &[EvalStmt::Return(Some(EvalExpr::Const(EvalConst::Int(i64::MIN))))], "{spelling}");
    }
}

/// Positive overflow and tightly binding suffixes cannot be reinterpreted as a negative literal.
#[test]
fn strict_integer_literal_eval_parser_rejects_positive_and_precedence_overflow() {
    for spelling in ["9223372036854775808", "9223372036854775809", "+9223372036854775808", "-(9223372036854775808)",
        "-9223372036854775808 ** 0", "-9223372036854775808[0]", "0x8000000000000001"] {
        assert_eq!(checked(&format!("return {spelling};")).unwrap_err(), EvalParseError::IntegerLiteralOverflow, "{spelling}");
    }
    let program = parse_fragment(b"return 9223372036854775808;").unwrap();
    assert!(matches!(program.statements(), [EvalStmt::Return(Some(EvalExpr::Const(EvalConst::Float(_))))]));
    for spelling in ["9223372036854775808.0", "9223372036854775808e0", "1e999"] {
        assert!(matches!(checked(&format!("return {spelling};")).unwrap().statements(),
            [EvalStmt::Return(Some(EvalExpr::Const(EvalConst::Float(_))))]));
    }
}

/// Recursive complex-string interpolation inherits checked lexing instead of using a default parser.
#[test]
fn strict_integer_literal_eval_interpolation_keeps_the_source_profile() {
    assert_eq!(checked("return \"{$items[9223372036854775809]}\";").unwrap_err(), EvalParseError::IntegerLiteralOverflow);
    assert!(checked("return \"$items[9223372036854775808]\";").is_ok());
}
