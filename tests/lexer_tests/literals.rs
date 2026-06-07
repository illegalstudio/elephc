//! Purpose:
//! Integration or regression tests for lexer tokenization coverage of literals, including echo string, escape sequences, and integer literal.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP source is tokenized and assertions check exact token kinds, literals, and source structure.

use super::*;

/// Verifies `source` produces a `FloatLiteral` token equal to `expected` within
/// floating-point epsilon tolerance. Used for overflow cases where the lexer
/// promotes integer literals to float (e.g., i64::MAX + 1).
fn assert_float_literal(source: &str, expected: f64) {
    let t = tokens(source);
    match &t[1] {
        Token::FloatLiteral(value) => {
            let tolerance = expected.abs() * f64::EPSILON;
            assert!(
                (*value - expected).abs() <= tolerance,
                "expected {expected}, got {value}"
            );
        }
        other => panic!("expected float literal, got {other:?}"),
    }
}

/// Verifies `echo "hello";` produces `OpenTag`, `Echo`, `StringLiteral("hello")`,
/// `Semicolon`, `Eof` — confirming echo statement parsing and string literal value extraction.
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

/// Verifies complex `{$var}` interpolation lexes the braced expression as a parenthesized
/// operand rather than emitting the braces as literal text.
#[test]
fn test_interpolation_complex_braces() {
    let t = tokens("<?php \"a{$b}c\";");
    assert_eq!(
        &t[1..t.len() - 2],
        &[
            Token::LParen,
            Token::StringLiteral("a".into()),
            Token::Dot,
            Token::LParen,
            Token::Variable("b".into()),
            Token::RParen,
            Token::Dot,
            Token::StringLiteral("c".into()),
            Token::RParen,
        ]
    );
}

/// Verifies simple `$arr[key]` interpolation lexes a bareword offset as a string-keyed
/// array access on the variable.
#[test]
fn test_interpolation_simple_offset_bareword() {
    let t = tokens("<?php \"$a[k]\";");
    assert_eq!(
        &t[1..t.len() - 2],
        &[
            Token::LParen,
            Token::StringLiteral(String::new()),
            Token::Dot,
            Token::Variable("a".into()),
            Token::LBracket,
            Token::StringLiteral("k".into()),
            Token::RBracket,
            Token::RParen,
        ]
    );
}

/// Verifies simple `$obj->prop` interpolation lexes a single property access on the variable.
#[test]
fn test_interpolation_simple_property() {
    let t = tokens("<?php \"$o->x\";");
    assert_eq!(
        &t[1..t.len() - 2],
        &[
            Token::LParen,
            Token::StringLiteral(String::new()),
            Token::Dot,
            Token::Variable("o".into()),
            Token::Arrow,
            Token::Identifier("x".into()),
            Token::RParen,
        ]
    );
}

/// Verifies double-quoted string `"hello\nworld\t!"` produces `StringLiteral`
/// with actual newline (`\n`) and tab (`\t`) characters — confirming escape sequence
/// interpretation, not raw literal text.
#[test]
fn test_string_escape_sequences() {
    let t = tokens("<?php \"hello\\nworld\\t!\"");
    assert_eq!(t[1], Token::StringLiteral("hello\nworld\t!".into()));
}

/// Verifies all PHP double-quoted escape sequences: `\r`, `\v`, `\e`, `\f`,
/// `\xHH`, `\x{...}`, `\u{...}`, and null byte `\0` — confirms unicode codepoint
/// conversion (`\u{1F600}` = 😀), lowercase hex decoding (`\x41` = `A`), and
/// C-style escape mapping (`\e` = U+001B, `\v` = U+000B).
#[test]
fn test_double_quoted_php_escape_sequences() {
    let t = tokens(r#"<?php "a\r\v\e\f\x41\101\u{1F600}\0""#);
    assert_eq!(
        t[1],
        Token::StringLiteral("a\r\u{0b}\u{1b}\u{0c}AA😀\0".into())
    );
}

/// Verifies out-of-bounds hex, invalid hex, and unrecognized escapes fall back to
/// literal text (matching PHP behavior).
#[test]
fn test_double_quoted_escape_digit_bounds_and_fallbacks() {
    let t = tokens(r#"<?php "\x414\1234\09\xG\u0041\'""#);
    assert_eq!(
        t[1],
        Token::StringLiteral("A4S4\09\\xG\\u0041\\'".into())
    );
}

/// Verifies bare decimal integer `42` tokenizes as `IntLiteral(42)`, not float.
#[test]
fn test_string_control_escape_sequences() {
    // \r, \v, \e, \f map to the matching ASCII control characters, matching
    // PHP double-quoted string semantics.
    let t = tokens("<?php \"\\r\\v\\e\\f\"");
    assert_eq!(
        t[1],
        Token::StringLiteral("\r\u{0B}\u{1B}\u{0C}".into()),
    );
}

/// Verifies lexer tokenization for integer literal.
#[test]
fn test_integer_literal() {
    let t = tokens("<?php 42");
    assert_eq!(t[1], Token::IntLiteral(42));
}

/// Verifies `i64::MAX` stays as integer (no overflow lex error).
#[test]
fn test_max_decimal_integer_literal_stays_int() {
    let t = tokens("<?php 9223372036854775807;");
    assert_eq!(t[1], Token::IntLiteral(i64::MAX));
}

/// Verifies `i64::MAX + 1` lexes as `FloatLiteral` (overflow promotes to float).
#[test]
fn test_decimal_integer_overflow_promotes_to_float() {
    assert_float_literal(
        "<?php 9223372036854775808;",
        9_223_372_036_854_775_808.0,
    );
}

/// Verifies PHP math constant tokens (`PHP_INT_MIN`, `M_PI`, `M_SQRT2`, etc.)
/// tokenize correctly.
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

/// Verifies `3.14` tokenizes as `FloatLiteral(3.14)`.
#[test]
fn test_float_literal() {
    let t = tokens("<?php 3.14");
    assert_eq!(t[1], Token::FloatLiteral(3.14));
}

/// Verifies `.5` (dot-prefix float without leading zero) tokenizes as
/// `FloatLiteral(0.5)`.
#[test]
fn test_dot_prefix_float() {
    let t = tokens("<?php .5");
    assert_eq!(t[1], Token::FloatLiteral(0.5));
}

/// Verifies `1.5e3` (lowercase `e` exponent) tokenizes as `FloatLiteral(1500.0)`.
#[test]
fn test_scientific_notation() {
    let t = tokens("<?php 1.5e3");
    assert_eq!(t[1], Token::FloatLiteral(1500.0));
}

/// Verifies `1.0e-5` tokenizes as `FloatLiteral(1.0e-5)`.
#[test]
fn test_scientific_notation_negative_exponent() {
    let t = tokens("<?php 1.0e-5");
    assert_eq!(t[1], Token::FloatLiteral(1.0e-5));
}

/// Verifies `42` does not tokenize as float (plain integer).
#[test]
fn test_integer_not_mistaken_for_float() {
    let t = tokens("<?php 42");
    assert_eq!(t[1], Token::IntLiteral(42));
}

/// Verifies `const NAME = "test";` produces `OpenTag`, `Const`, `Identifier`,
/// `Assign`, `StringLiteral("test")`, `Semicolon`, `Eof` — confirming const
/// declaration parsing and string literal extraction.
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

/// Verifies `0xff` (lowercase `x`) hex literal tokenizes as `IntLiteral(255)`.
#[test]
fn test_hex_literal_lowercase() {
    let t = tokens("<?php 0xff;");
    assert_eq!(t[1], Token::IntLiteral(255));
}

/// Verifies `0XFF` (uppercase `X` prefix) hex literal tokenizes as `IntLiteral(255)`
/// — confirming uppercase variant is accepted.
#[test]
fn test_hex_literal_uppercase_x() {
    let t = tokens("<?php 0XFF;");
    assert_eq!(t[1], Token::IntLiteral(255));
}

/// Verifies hex digits are case-insensitive (`0x1aB` = 427).
#[test]
fn test_hex_literal_mixed_case_digits() {
    let t = tokens("<?php 0x1aB;");
    assert_eq!(t[1], Token::IntLiteral(427));
}

/// Verifies `0x0` (zero) tokenizes correctly.
#[test]
fn test_hex_literal_zero() {
    let t = tokens("<?php 0x0;");
    assert_eq!(t[1], Token::IntLiteral(0));
}

/// Verifies `0xFFFFFFFFFFFFFFFF` overflows to `FloatLiteral`.
#[test]
fn test_hex_integer_overflow_promotes_to_float() {
    assert_float_literal(
        "<?php 0xFFFFFFFFFFFFFFFF;",
        18_446_744_073_709_551_616.0,
    );
}

// --- Octal integer literals ---

/// Verifies `0o777` (lowercase `o`) explicit octal tokenizes as `IntLiteral(511)`.
#[test]
fn test_explicit_octal_literal_lowercase() {
    let t = tokens("<?php 0o777;");
    assert_eq!(t[1], Token::IntLiteral(511));
}

/// Verifies `0O777` (uppercase `O` prefix) explicit octal tokenizes as `IntLiteral(511)`
/// — confirming uppercase variant is accepted.
#[test]
fn test_explicit_octal_literal_uppercase_o() {
    let t = tokens("<?php 0O777;");
    assert_eq!(t[1], Token::IntLiteral(511));
}

/// Verifies `0o0` (zero) explicit octal tokenizes correctly.
#[test]
fn test_explicit_octal_literal_zero() {
    let t = tokens("<?php 0o0;");
    assert_eq!(t[1], Token::IntLiteral(0));
}

/// Verifies legacy octal `0777` (no prefix) tokenizes as `IntLiteral(511)`.
#[test]
fn test_legacy_octal_literal() {
    let t = tokens("<?php 0777;");
    assert_eq!(t[1], Token::IntLiteral(511));
}

/// Verifies `0_777` (leading-zero octal with separator) tokenizes as `IntLiteral(511)`
/// — confirming underscore separator is accepted inside a legacy octal literal.
#[test]
fn test_legacy_octal_literal_with_separator() {
    let t = tokens("<?php 0_777;");
    assert_eq!(t[1], Token::IntLiteral(511));
}

/// Verifies `0o` prefix + overflow promotes to `FloatLiteral`.
#[test]
fn test_explicit_octal_integer_overflow_promotes_to_float() {
    assert_float_literal(
        "<?php 0o1777777777777777777777;",
        18_446_744_073_709_551_616.0,
    );
}

/// Verifies legacy octal + overflow promotes to `FloatLiteral`.
#[test]
fn test_legacy_octal_integer_overflow_promotes_to_float() {
    assert_float_literal(
        "<?php 01777777777777777777777;",
        18_446_744_073_709_551_616.0,
    );
}

/// Verifies `012.3` (leading zero float) stays decimal 12.3, not octal.
#[test]
fn test_leading_zero_float_stays_decimal() {
    let t = tokens("<?php 012.3;");
    assert_eq!(t[1], Token::FloatLiteral(12.3));
}

/// Verifies `08e1` stays decimal 80.0 (not octal), matching PHP behavior.
#[test]
fn test_leading_zero_scientific_float_stays_decimal() {
    let t = tokens("<?php 08e1;");
    assert_eq!(t[1], Token::FloatLiteral(80.0));
}

// --- Binary integer literals ---

/// Verifies `0b1010` (lowercase `b`) binary literal tokenizes as `IntLiteral(10)`.
#[test]
fn test_binary_literal_lowercase() {
    let t = tokens("<?php 0b1010;");
    assert_eq!(t[1], Token::IntLiteral(10));
}

/// Verifies `0B1010` (uppercase `B` prefix) binary literal tokenizes as `IntLiteral(10)`
/// — confirming uppercase variant is accepted.
#[test]
fn test_binary_literal_uppercase_b() {
    let t = tokens("<?php 0B1010;");
    assert_eq!(t[1], Token::IntLiteral(10));
}

/// Verifies `0b0` binary zero tokenizes correctly.
#[test]
fn test_binary_literal_zero() {
    let t = tokens("<?php 0b0;");
    assert_eq!(t[1], Token::IntLiteral(0));
}

/// Verifies `0b1` binary one tokenizes correctly.
#[test]
fn test_binary_literal_one() {
    let t = tokens("<?php 0b1;");
    assert_eq!(t[1], Token::IntLiteral(1));
}

/// Verifies `0b11111111` (8 bits) tokenizes as `IntLiteral(255)`.
#[test]
fn test_binary_literal_eight_bits() {
    let t = tokens("<?php 0b11111111;");
    assert_eq!(t[1], Token::IntLiteral(255));
}

/// Verifies 64-bit binary overflow promotes to `FloatLiteral`.
#[test]
fn test_binary_integer_overflow_promotes_to_float() {
    assert_float_literal(
        "<?php 0b1111111111111111111111111111111111111111111111111111111111111111;",
        18_446_744_073_709_551_616.0,
    );
}

// --- Numeric separators ---

/// Verifies `1_000_000` decimal with separators tokenizes as `IntLiteral(1_000_000)`.
#[test]
fn test_decimal_separator() {
    let t = tokens("<?php 1_000_000;");
    assert_eq!(t[1], Token::IntLiteral(1_000_000));
}

/// Verifies `0xFF_FF` hex with separators tokenizes as `IntLiteral(0xFFFF)`.
#[test]
fn test_hex_separator() {
    let t = tokens("<?php 0xFF_FF;");
    assert_eq!(t[1], Token::IntLiteral(0xFFFF));
}

/// Verifies `0o7_7_7` explicit octal with separators tokenizes as `IntLiteral(0o777)`.
#[test]
fn test_explicit_octal_separator() {
    let t = tokens("<?php 0o7_7_7;");
    assert_eq!(t[1], Token::IntLiteral(0o777));
}

/// Verifies `0b1010_1010` binary with separators tokenizes correctly.
#[test]
fn test_binary_separator() {
    let t = tokens("<?php 0b1010_1010;");
    assert_eq!(t[1], Token::IntLiteral(0b10101010));
}

/// Verifies `1_000.5` (separator in integer part) tokenizes as `FloatLiteral(1000.5)`.
#[test]
fn test_float_separator_int_part() {
    let t = tokens("<?php 1_000.5;");
    assert_eq!(t[1], Token::FloatLiteral(1000.5));
}

/// Verifies `1.5_5` (separator in fractional part) tokenizes as `FloatLiteral(1.55)`.
#[test]
fn test_float_separator_frac_part() {
    let t = tokens("<?php 1.5_5;");
    assert_eq!(t[1], Token::FloatLiteral(1.55));
}

/// Verifies `1e1_0` (separator in exponent) tokenizes as `FloatLiteral(1e10)`.
#[test]
fn test_float_separator_exponent() {
    let t = tokens("<?php 1e1_0;");
    assert_eq!(t[1], Token::FloatLiteral(1e10));
}

/// Verifies `1e+1_0` (signed exponent with separator) tokenizes as `FloatLiteral(1e10)`.
#[test]
fn test_float_separator_signed_exp() {
    let t = tokens("<?php 1e+1_0;");
    assert_eq!(t[1], Token::FloatLiteral(1e10));
}
