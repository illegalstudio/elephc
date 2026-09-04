//! Purpose:
//! Pins the token shapes the double-quoted string lexer produces.
//!
//! Before this suite existed the eval interpreter's lexer returned every double-quoted
//! literal as one opaque `TokenKind::String`, so a runtime-interpreted fragment printed
//! `a:$i:` where PHP 8.5.6 prints `a:7:`. These cases pin both halves of the contract:
//! a literal WITHOUT interpolation must still be exactly one `String` token, and a
//! literal WITH interpolation must be a parenthesized `.` concatenation.
//!
//! Called from:
//! - `cargo test -p elephc-magician --lib lexer::tests` through Rust's test harness.
//!
//! Key details:
//! - Assertions compare the WHOLE token vector, never a "contains" probe, so a stray
//!   extra or missing token fails.
//! - Malformed simple offsets are PHP parse errors (measured against PHP 8.5.6) and are
//!   pinned as refusals so a future rewrite cannot start silently accepting them.

use super::scan::tokenize;
use super::TokenKind;
use crate::errors::EvalParseError;

/// Tokenizes a fragment and returns the token kinds, failing the test on a parse error.
fn kinds(source: &str) -> Vec<TokenKind> {
    tokenize(source)
        .expect("fragment should tokenize")
        .into_iter()
        .map(super::Token::into_kind)
        .collect()
}

/// Returns the parse error a fragment produces, failing the test if it tokenizes.
fn error(source: &str) -> EvalParseError {
    tokenize(source).expect_err("fragment should be refused")
}

/// Builds a `TokenKind::String` from a literal for terser expectations.
fn string(value: &str) -> TokenKind {
    TokenKind::String(value.to_string())
}

/// Builds a `TokenKind::DollarIdent` from a literal for terser expectations.
fn var(name: &str) -> TokenKind {
    TokenKind::DollarIdent(name.to_string())
}

/// Verifies a double-quoted literal without interpolation stays exactly ONE string token.
///
/// This is the invariant that keeps constant initializers, property defaults, enum
/// backing values and `declare()` arguments behaving identically: `parser::primary` maps
/// a lone `TokenKind::String` straight to `EvalConst::String`, and a parenthesized concat
/// would change that shape for every plain literal in every fragment.
#[test]
fn plain_double_quoted_literal_stays_one_string_token() {
    assert_eq!(kinds(r#""plain";"#), vec![string("plain"), TokenKind::Semicolon, TokenKind::Eof]);
}

/// Verifies single-quoted literals never interpolate and stay one token.
#[test]
fn single_quoted_literal_never_interpolates() {
    assert_eq!(
        kinds(r#"'$v and {$v}';"#),
        vec![string("$v and {$v}"), TokenKind::Semicolon, TokenKind::Eof]
    );
}

/// Verifies `\$` yields a literal `$` and suppresses interpolation, still as one token.
#[test]
fn escaped_dollar_stays_literal_and_suppresses_interpolation() {
    assert_eq!(
        kinds(r#""\$v";"#),
        vec![string("$v"), TokenKind::Semicolon, TokenKind::Eof]
    );
}

/// Verifies simple, hexadecimal, and octal double-quoted escapes.
#[test]
fn double_quoted_escape_table_is_unchanged() {
    assert_eq!(
        kinds(r#""a\nb\tc\\d\"e\q\x41\101\0";"#),
        vec![
            string("a\nb\tc\\d\"e\\qAA\0"),
            TokenKind::Semicolon,
            TokenKind::Eof
        ]
    );
}

/// Verifies a simple `$name` expands to a parenthesized concatenation.
///
/// The leading empty string literal is deliberate: it is what makes the resulting `.`
/// chain string-typed, matching PHP's rule that a double-quoted literal is always a
/// string even when it holds nothing but an integer variable.
#[test]
fn simple_variable_expands_to_parenthesized_concat() {
    assert_eq!(
        kinds(r#""$v";"#),
        vec![
            TokenKind::LParen,
            string(""),
            TokenKind::Dot,
            var("v"),
            TokenKind::RParen,
            TokenKind::Semicolon,
            TokenKind::Eof,
        ]
    );
}

/// Verifies literal text around an interpolation is concatenated on both sides.
#[test]
fn surrounding_literal_text_is_concatenated() {
    assert_eq!(
        kinds(r#""pre-$v-post";"#),
        vec![
            TokenKind::LParen,
            string("pre-"),
            TokenKind::Dot,
            var("v"),
            TokenKind::Dot,
            string("-post"),
            TokenKind::RParen,
            TokenKind::Semicolon,
            TokenKind::Eof,
        ]
    );
}

/// Verifies a bare `$a[key]` offset becomes a string key, which is PHP's simple syntax.
#[test]
fn bare_offset_key_becomes_a_string_key() {
    assert_eq!(
        kinds(r#""$a[k]";"#),
        vec![
            TokenKind::LParen,
            string(""),
            TokenKind::Dot,
            var("a"),
            TokenKind::LBracket,
            string("k"),
            TokenKind::RBracket,
            TokenKind::RParen,
            TokenKind::Semicolon,
            TokenKind::Eof,
        ]
    );
}

/// Verifies a negative simple offset lexes as one negative integer, not unary minus.
#[test]
fn negative_offset_key_becomes_one_negative_integer() {
    assert_eq!(
        kinds(r#""$a[-1]";"#),
        vec![
            TokenKind::LParen,
            string(""),
            TokenKind::Dot,
            var("a"),
            TokenKind::LBracket,
            TokenKind::Int(-1),
            TokenKind::RBracket,
            TokenKind::RParen,
            TokenKind::Semicolon,
            TokenKind::Eof,
        ]
    );
}

/// Verifies a variable simple offset keeps the variable rather than a bareword key.
#[test]
fn variable_offset_key_stays_a_variable() {
    assert_eq!(
        kinds(r#""$a[$k]";"#),
        vec![
            TokenKind::LParen,
            string(""),
            TokenKind::Dot,
            var("a"),
            TokenKind::LBracket,
            var("k"),
            TokenKind::RBracket,
            TokenKind::RParen,
            TokenKind::Semicolon,
            TokenKind::Eof,
        ]
    );
}

/// Verifies `$obj->prop` simple syntax emits the property access tokens.
#[test]
fn simple_property_access_is_interpolated() {
    assert_eq!(
        kinds(r#""$o->p";"#),
        vec![
            TokenKind::LParen,
            string(""),
            TokenKind::Dot,
            var("o"),
            TokenKind::Arrow,
            TokenKind::Ident("p".to_string()),
            TokenKind::RParen,
            TokenKind::Semicolon,
            TokenKind::Eof,
        ]
    );
}

/// Verifies `->` followed by a digit is literal text, matching PHP 8.5.6.
///
/// Measured: `"$o->1"` interpolates `$o` alone and keeps `->1` as text (it fails with
/// "Object of class Obj could not be converted to string", proving the object itself was
/// the interpolated value). A property name may not start with a digit.
#[test]
fn arrow_followed_by_a_digit_is_literal_text() {
    assert_eq!(
        kinds(r#""$o->1";"#),
        vec![
            TokenKind::LParen,
            string(""),
            TokenKind::Dot,
            var("o"),
            TokenKind::Dot,
            string("->1"),
            TokenKind::RParen,
            TokenKind::Semicolon,
            TokenKind::Eof,
        ]
    );
}

/// Verifies complex `{$expr}` interpolation nests the fragment inside its own parens.
#[test]
fn complex_interpolation_nests_the_fragment() {
    assert_eq!(
        kinds(r#""{$a['k']}";"#),
        vec![
            TokenKind::LParen,
            string(""),
            TokenKind::Dot,
            TokenKind::LParen,
            var("a"),
            TokenKind::LBracket,
            string("k"),
            TokenKind::RBracket,
            TokenKind::RParen,
            TokenKind::RParen,
            TokenKind::Semicolon,
            TokenKind::Eof,
        ]
    );
}

/// Verifies a `{` not followed by `$` stays literal text, matching PHP's `"{ $x }"`.
#[test]
fn brace_without_dollar_is_literal_text() {
    assert_eq!(
        kinds(r#""{ $s }";"#),
        vec![
            TokenKind::LParen,
            string("{ "),
            TokenKind::Dot,
            var("s"),
            TokenKind::Dot,
            string(" }"),
            TokenKind::RParen,
            TokenKind::Semicolon,
            TokenKind::Eof,
        ]
    );
}

/// Verifies the legacy `${name}` form interpolates the named variable.
#[test]
fn legacy_dollar_brace_form_interpolates() {
    assert_eq!(
        kinds(r#""${v}";"#),
        vec![
            TokenKind::LParen,
            string(""),
            TokenKind::Dot,
            TokenKind::LParen,
            var("v"),
            TokenKind::RParen,
            TokenKind::RParen,
            TokenKind::Semicolon,
            TokenKind::Eof,
        ]
    );
}

/// Verifies `$` followed by a digit is literal text and the literal stays ONE token.
///
/// Measured on PHP 8.5.6: `echo "$2-$1";` prints `$2-$1`, because a PHP variable name
/// cannot start with a digit. This is not academic — it is exactly how `preg_replace()`
/// back-references are written inside a double-quoted replacement, so treating `$2` as a
/// variable silently breaks every such call. The magician's general `lex_ident()` DOES
/// accept a leading digit, so the interpolation path must gate on `is_ident_start`.
#[test]
fn dollar_followed_by_a_digit_is_literal_text() {
    assert_eq!(
        kinds(r#""$2-$1";"#),
        vec![string("$2-$1"), TokenKind::Semicolon, TokenKind::Eof]
    );
}

/// Verifies a digit-led simple offset variable is refused rather than read as a variable.
///
/// PHP 8.5.6 on `"$a[$2]"`: `syntax error, unexpected token "$"`.
#[test]
fn digit_led_offset_variable_is_refused() {
    assert_eq!(error(r#""$a[$2]";"#), EvalParseError::ExpectedVariable);
}

/// Verifies `$` with no identifier after it is literal text, so `"$$s"` yields `$` + `$s`.
#[test]
fn dollar_without_identifier_is_literal_text() {
    assert_eq!(
        kinds(r#""$$s";"#),
        vec![
            TokenKind::LParen,
            string("$"),
            TokenKind::Dot,
            var("s"),
            TokenKind::RParen,
            TokenKind::Semicolon,
            TokenKind::Eof,
        ]
    );
}

/// Verifies every synthetic token of a multi-line literal carries the opening-quote line
/// and that the token after the literal resumes at the real line.
///
/// `__LINE__` inside a fragment reads these numbers, so an interpolation that stamped the
/// closing-quote line would silently shift diagnostics.
#[test]
fn interpolated_tokens_carry_the_opening_quote_line() {
    let tokens = tokenize("$x;\n\"a\n$v\nb\";\n$y;").expect("fragment should tokenize");
    let lines: Vec<i64> = tokens.iter().map(super::Token::line).collect();
    // `$x` `;` on line 1, then the literal's nine tokens
    // (`(` `"a\n"` `.` `$v` `.` `"\nb"` `)`) all stamped with its opening-quote line 2,
    // the `;` after the closing quote on line 4, then `$y` `;` on line 5 and EOF.
    assert_eq!(
        kinds("$x;\n\"a\n$v\nb\";\n$y;"),
        vec![
            var("x"),
            TokenKind::Semicolon,
            TokenKind::LParen,
            string("a\n"),
            TokenKind::Dot,
            var("v"),
            TokenKind::Dot,
            string("\nb"),
            TokenKind::RParen,
            TokenKind::Semicolon,
            var("y"),
            TokenKind::Semicolon,
            TokenKind::Eof,
        ]
    );
    assert_eq!(lines, vec![1, 1, 2, 2, 2, 2, 2, 2, 2, 4, 5, 5, 5]);
}

/// Verifies `"$a[]"` is refused. PHP 8.5.6: `syntax error, unexpected token "]"`.
///
/// The main compiler lexer accepts this and invents an empty-string key; the port must
/// not reproduce that over-acceptance.
#[test]
fn empty_simple_offset_is_refused() {
    assert_eq!(error(r#""$a[]";"#), EvalParseError::UnexpectedToken);
}

/// Verifies `"$a['k']"` is refused. PHP 8.5.6 rejects quoted keys in simple syntax.
#[test]
fn quoted_simple_offset_is_refused() {
    assert_eq!(error(r#""$a['k']";"#), EvalParseError::UnexpectedToken);
}

/// Verifies `"$a[ 0]"` is refused. PHP 8.5.6 rejects whitespace in simple syntax offsets.
#[test]
fn spaced_simple_offset_is_refused() {
    assert_eq!(error(r#""$a[ 0]";"#), EvalParseError::UnexpectedToken);
}

/// Verifies `"$a[-]"` is refused. PHP 8.5.6: `syntax error, unexpected token "]"`.
#[test]
fn lone_minus_simple_offset_is_refused() {
    assert_eq!(error(r#""$a[-]";"#), EvalParseError::InvalidNumber);
}

/// Verifies a simple offset that never closes is refused rather than silently truncated.
#[test]
fn unterminated_simple_offset_is_refused() {
    assert_eq!(error(r#""$a[0";"#), EvalParseError::UnterminatedString);
}

/// Verifies an unterminated `{$...}` is refused.
#[test]
fn unterminated_complex_interpolation_is_refused() {
    assert_eq!(error(r#""{$v";"#), EvalParseError::UnterminatedString);
}

/// Verifies errors from the recursive brace tokenizer propagate instead of being swallowed.
///
/// Swallowing them would turn arbitrary garbage inside `{...}` into silently accepted
/// text, which is the over-acceptance this port is most exposed to.
#[test]
fn garbage_inside_braces_propagates_the_inner_error() {
    assert_eq!(error(r#""{$ }";"#), EvalParseError::ExpectedVariable);
}

/// Verifies an unterminated double-quoted literal is still refused.
#[test]
fn unterminated_double_quoted_literal_is_refused() {
    assert_eq!(error(r#""abc"#), EvalParseError::UnterminatedString);
}

/// Verifies a nested literal inside `{$...}` is captured whole, braces and quotes included.
#[test]
fn nested_literal_inside_braces_is_captured_verbatim() {
    assert_eq!(
        kinds(r#""{$a['}{']}";"#),
        vec![
            TokenKind::LParen,
            string(""),
            TokenKind::Dot,
            TokenKind::LParen,
            var("a"),
            TokenKind::LBracket,
            string("}{"),
            TokenKind::RBracket,
            TokenKind::RParen,
            TokenKind::RParen,
            TokenKind::Semicolon,
            TokenKind::Eof,
        ]
    );
}
