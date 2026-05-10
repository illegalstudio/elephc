//! Purpose:
//! Integration or regression tests for lexer tokenization coverage of literals, including echo string, escape sequences, and integer literal.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP source is tokenized and assertions check exact token kinds, literals, and source structure.

use super::*;

#[test]
fn test_echo_string() {
    let t = tokens("<?php echo \"hello\";");
    assert_eq!(
        t,
        vec![
            Token::OpenTag,
            Token::Echo,
            Token::StringLiteral("hello".into()),
            Token::Semicolon,
            Token::Eof,
        ]
    );
}

#[test]
fn test_string_escape_sequences() {
    let t = tokens("<?php \"hello\\nworld\\t!\"");
    assert_eq!(t[1], Token::StringLiteral("hello\nworld\t!".into()));
}

#[test]
fn test_integer_literal() {
    let t = tokens("<?php 42");
    assert_eq!(t[1], Token::IntLiteral(42));
}

#[test]
fn test_additional_numeric_constant_tokens() {
    let t = tokens(
        "<?php PHP_INT_MIN PHP_FLOAT_MAX PHP_FLOAT_MIN PHP_FLOAT_EPSILON M_E M_SQRT2 M_PI_2 M_PI_4 M_LOG2E M_LOG10E",
    );
    assert_eq!(
        t[1..11],
        [
            Token::PhpIntMin,
            Token::PhpFloatMax,
            Token::PhpFloatMin,
            Token::PhpFloatEpsilon,
            Token::ME,
            Token::MSqrt2,
            Token::MPi2,
            Token::MPi4,
            Token::MLog2e,
            Token::MLog10e,
        ]
    );
}

// --- INF / NAN ---

#[test]
fn test_float_literal() {
    let t = tokens("<?php 3.14");
    assert_eq!(t[1], Token::FloatLiteral(3.14));
}

#[test]
fn test_dot_prefix_float() {
    let t = tokens("<?php .5");
    assert_eq!(t[1], Token::FloatLiteral(0.5));
}

#[test]
fn test_scientific_notation() {
    let t = tokens("<?php 1.5e3");
    assert_eq!(t[1], Token::FloatLiteral(1500.0));
}

#[test]
fn test_scientific_notation_negative_exponent() {
    let t = tokens("<?php 1.0e-5");
    assert_eq!(t[1], Token::FloatLiteral(1.0e-5));
}

#[test]
fn test_integer_not_mistaken_for_float() {
    let t = tokens("<?php 42");
    assert_eq!(t[1], Token::IntLiteral(42));
}

#[test]
fn test_const_string_value() {
    let t = tokens("<?php const NAME = \"test\";");
    assert_eq!(
        t,
        vec![
            Token::OpenTag,
            Token::Const,
            Token::Identifier("NAME".into()),
            Token::Assign,
            Token::StringLiteral("test".into()),
            Token::Semicolon,
            Token::Eof,
        ]
    );
}

// --- Hex integer literals ---

#[test]
fn test_hex_literal_lowercase() {
    let t = tokens("<?php 0xff;");
    assert_eq!(t[1], Token::IntLiteral(255));
}

#[test]
fn test_hex_literal_uppercase_x() {
    let t = tokens("<?php 0XFF;");
    assert_eq!(t[1], Token::IntLiteral(255));
}

#[test]
fn test_hex_literal_mixed_case_digits() {
    let t = tokens("<?php 0x1aB;");
    assert_eq!(t[1], Token::IntLiteral(427));
}

#[test]
fn test_hex_literal_zero() {
    let t = tokens("<?php 0x0;");
    assert_eq!(t[1], Token::IntLiteral(0));
}

// --- Octal integer literals ---

#[test]
fn test_explicit_octal_literal_lowercase() {
    let t = tokens("<?php 0o777;");
    assert_eq!(t[1], Token::IntLiteral(511));
}

#[test]
fn test_explicit_octal_literal_uppercase_o() {
    let t = tokens("<?php 0O777;");
    assert_eq!(t[1], Token::IntLiteral(511));
}

#[test]
fn test_explicit_octal_literal_zero() {
    let t = tokens("<?php 0o0;");
    assert_eq!(t[1], Token::IntLiteral(0));
}

#[test]
fn test_legacy_octal_literal() {
    let t = tokens("<?php 0777;");
    assert_eq!(t[1], Token::IntLiteral(511));
}

#[test]
fn test_legacy_octal_literal_with_separator() {
    let t = tokens("<?php 0_777;");
    assert_eq!(t[1], Token::IntLiteral(511));
}

#[test]
fn test_leading_zero_float_stays_decimal() {
    let t = tokens("<?php 012.3;");
    assert_eq!(t[1], Token::FloatLiteral(12.3));
}

#[test]
fn test_leading_zero_scientific_float_stays_decimal() {
    let t = tokens("<?php 08e1;");
    assert_eq!(t[1], Token::FloatLiteral(80.0));
}

// --- Binary integer literals ---

#[test]
fn test_binary_literal_lowercase() {
    let t = tokens("<?php 0b1010;");
    assert_eq!(t[1], Token::IntLiteral(10));
}

#[test]
fn test_binary_literal_uppercase_b() {
    let t = tokens("<?php 0B1010;");
    assert_eq!(t[1], Token::IntLiteral(10));
}

#[test]
fn test_binary_literal_zero() {
    let t = tokens("<?php 0b0;");
    assert_eq!(t[1], Token::IntLiteral(0));
}

#[test]
fn test_binary_literal_one() {
    let t = tokens("<?php 0b1;");
    assert_eq!(t[1], Token::IntLiteral(1));
}

#[test]
fn test_binary_literal_eight_bits() {
    let t = tokens("<?php 0b11111111;");
    assert_eq!(t[1], Token::IntLiteral(255));
}

// --- Numeric separators ---

#[test]
fn test_decimal_separator() {
    let t = tokens("<?php 1_000_000;");
    assert_eq!(t[1], Token::IntLiteral(1_000_000));
}

#[test]
fn test_hex_separator() {
    let t = tokens("<?php 0xFF_FF;");
    assert_eq!(t[1], Token::IntLiteral(0xFFFF));
}

#[test]
fn test_explicit_octal_separator() {
    let t = tokens("<?php 0o7_7_7;");
    assert_eq!(t[1], Token::IntLiteral(0o777));
}

#[test]
fn test_binary_separator() {
    let t = tokens("<?php 0b1010_1010;");
    assert_eq!(t[1], Token::IntLiteral(0b10101010));
}

#[test]
fn test_float_separator_int_part() {
    let t = tokens("<?php 1_000.5;");
    assert_eq!(t[1], Token::FloatLiteral(1000.5));
}

#[test]
fn test_float_separator_frac_part() {
    let t = tokens("<?php 1.5_5;");
    assert_eq!(t[1], Token::FloatLiteral(1.55));
}

#[test]
fn test_float_separator_exponent() {
    let t = tokens("<?php 1e1_0;");
    assert_eq!(t[1], Token::FloatLiteral(1e10));
}

#[test]
fn test_float_separator_signed_exp() {
    let t = tokens("<?php 1e+1_0;");
    assert_eq!(t[1], Token::FloatLiteral(1e10));
}
