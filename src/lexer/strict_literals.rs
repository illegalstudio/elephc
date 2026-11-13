//! Purpose:
//! Tests checked integer spelling and signed-minimum parsing through the public lexer API.
//!
//! Called from:
//! - Focused compiler library tests.
//!
//! Key details:
//! - Default tokenization remains PHP-compatible; explicit float spellings remain floats.

use super::*;
use crate::parser::ast::{ExprKind, StmtKind};
use elephc_semantics::{SemanticProfile, StrictFeature};

/// Builds a checked token stream with an included-file source identity.
fn checked(source: &str) -> Result<Vec<SpannedToken>, CompileError> {
    tokenize_with_semantics(source, SourceMode::Php, 7,
        SemanticProfile::PERMISSIVE.with(StrictFeature::Overflow, true))
}

/// The lexer retains an exact candidate for the minimum signed value and rejects larger integers.
#[test]
fn strict_integer_literal_compiler_tokens_preserve_boundary_magnitude() {
    let tokens = checked("<?php echo 9223372036854775808;").unwrap();
    assert!(tokens.iter().any(|(token, span)| *token == Token::MinimumIntegerMagnitude && span.span.source_id() == 7));
    assert!(tokenize("<?php echo 9223372036854775808;").unwrap().iter()
        .any(|(token, _)| matches!(token, Token::FloatLiteral(_))));
    for literal in ["9223372036854775809", "18446744073709551616", "0x8000000000000001", "0xffffffffffffffffffffffff"] {
        let error = checked(&format!("<?php echo {literal};")).unwrap_err();
        assert!(error.message.contains("--strict-overflow"), "{literal}");
        assert_eq!(error.span.source_id(), 7);
        assert_eq!(error.span.col, 12);
    }
    for literal in ["9223372036854775808.0", "9223372036854775808e0", "1e999"] {
        assert!(checked(&format!("<?php echo {literal};")).unwrap().iter()
            .any(|(token, _)| matches!(token, Token::FloatLiteral(_))));
    }
}

/// Unary minimum spellings remain exact and power/postfix precedence cannot hide a positive overflow.
#[test]
fn strict_integer_literal_compiler_parser_requires_a_direct_negative_magnitude() {
    for literal in ["-9223372036854775808", "-9_223_372_036_854_775_808", "-0x8000000000000000", "-0o1000000000000000000000"] {
        let ast = crate::parser::parse_with_mode(&checked(&format!("<?php echo {literal};")).unwrap(), SourceMode::Php).unwrap();
        assert!(matches!(&ast[0].kind, StmtKind::Echo(expr) if expr.kind == ExprKind::IntLiteral(i64::MIN)), "{literal}");
    }
    for literal in ["9223372036854775808", "+9223372036854775808", "-(9223372036854775808)", "-9223372036854775808 ** 0", "-9223372036854775808[0]"] {
        let error = crate::parser::parse_with_mode(&checked(&format!("<?php echo {literal};")).unwrap(), SourceMode::Php).unwrap_err();
        assert!(error.message.contains("strict-overflow"), "{literal}: {error:?}");
    }
}

/// Nested interpolation tokenization must use the same profile as the physical file.
#[test]
fn strict_integer_literal_compiler_interpolation_keeps_the_source_profile() {
    for source in [
        "<?php echo \"{$items[9223372036854775809]}\";",
        "<?php echo <<<TEXT\n{$items[9223372036854775809]}\nTEXT;",
    ] {
        assert!(checked(source).unwrap_err().message.contains("strict-overflow"));
    }
    let tokens = checked("<?php echo \"$items[9223372036854775808]\";").unwrap();
    assert!(tokens.iter().any(|(token, _)| matches!(token, Token::StringLiteral(value) if value == "9223372036854775808")));
    for exact in [false, true] {
        let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::ArrayKeys, exact);
        let tokens = tokenize_with_semantics("<?php echo \"$items[1]\";", SourceMode::Php, 7, profile).unwrap();
        assert!(tokens.iter().any(|(token, _)| if exact {
            matches!(token, Token::StringLiteral(value) if value == "1")
        } else { matches!(token, Token::IntLiteral(1)) }));
    }

}
