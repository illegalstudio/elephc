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

/// Tokenizes a lone numeric literal and returns the single token kind it produced.
///
/// Every accepted numeric literal must lex to EXACTLY one token followed by `Eof`;
/// the bug this guards against split `0x1F` into `Int(0)` plus `Ident("x1F")`, which
/// is why the arity is asserted here rather than in each case.
fn number(source: &str) -> TokenKind {
    let mut tokens = kinds(source);
    assert_eq!(
        &tokens[1..],
        [TokenKind::Semicolon, TokenKind::Eof],
        "numeric literal {source:?} should lex to exactly one token before `;`, got {tokens:?}"
    );
    tokens.remove(0)
}

// --- Numeric literals ---

/// Verifies a legacy leading-`0` octal literal is read in base 8, not base 10.
///
/// Measured against PHP 8.5.6: `echo 0700;` prints `448` and `echo 0100644;` prints
/// `33188`. The eval lexer scanned decimal digits and called `parse::<i64>()` on the
/// raw text, so it printed `700` and `100644` — a silent wrong answer that made every
/// Unix permission mask in an `eval()`d fragment mean something else.
#[test]
fn legacy_leading_zero_literal_is_octal() {
    assert_eq!(number("0700;"), TokenKind::Int(448));
    assert_eq!(number("0100644;"), TokenKind::Int(33188));
    assert_eq!(number("010;"), TokenKind::Int(8));
}

/// Verifies the PHP 8.1+ explicit `0o`/`0O` octal prefix. PHP 8.5.6: both print `448`.
#[test]
fn explicit_octal_prefix_is_octal_in_both_cases() {
    assert_eq!(number("0o700;"), TokenKind::Int(448));
    assert_eq!(number("0O700;"), TokenKind::Int(448));
}

/// Verifies hex literals in both prefix cases. PHP 8.5.6: `0x1F` and `0X1f` print `31`.
#[test]
fn hex_prefix_is_base_sixteen_in_both_cases() {
    assert_eq!(number("0x1F;"), TokenKind::Int(31));
    assert_eq!(number("0X1f;"), TokenKind::Int(31));
}

/// Verifies binary literals in both prefix cases. PHP 8.5.6: `0b101` and `0B101` print `5`.
#[test]
fn binary_prefix_is_base_two_in_both_cases() {
    assert_eq!(number("0b101;"), TokenKind::Int(5));
    assert_eq!(number("0B101;"), TokenKind::Int(5));
}

/// Verifies PHP 7.4+ `_` digit separators are accepted and stripped in every base.
///
/// PHP 8.5.6: `1_000` → `1000`, `0_700` → `448`, `0o7_00` → `448`, `0x1_F` → `31`,
/// `0b1_01` → `5`. The eval lexer stopped at the `_` and lexed the rest as an
/// identifier, so `1_000` became `Int(1)` followed by `Ident("_000")`.
#[test]
fn underscore_separators_are_stripped_in_every_base() {
    assert_eq!(number("1_000;"), TokenKind::Int(1000));
    assert_eq!(number("0_700;"), TokenKind::Int(448));
    assert_eq!(number("0o7_00;"), TokenKind::Int(448));
    assert_eq!(number("0x1_F;"), TokenKind::Int(31));
    assert_eq!(number("0b1_01;"), TokenKind::Int(5));
}

/// Verifies a bare `0` and a repeated `00` stay zero rather than being mistaken for a
/// prefix. PHP 8.5.6: both print `0`.
#[test]
fn zero_literals_stay_zero() {
    assert_eq!(number("0;"), TokenKind::Int(0));
    assert_eq!(number("00;"), TokenKind::Int(0));
}

/// Verifies scientific notation produces a float. PHP 8.5.6: `1e3` and `1E3` print
/// `1000`, `1.5e2` prints `150`, `1e-3` prints `0.001`.
#[test]
fn scientific_notation_is_a_float() {
    assert_eq!(number("1e3;"), TokenKind::Float(1000.0));
    assert_eq!(number("1E3;"), TokenKind::Float(1000.0));
    assert_eq!(number("1.5e2;"), TokenKind::Float(150.0));
    assert_eq!(number("1e-3;"), TokenKind::Float(0.001));
}

/// Verifies a float keeps its separators and decimal part. PHP 8.5.6: `1_000.5` → `1000.5`.
#[test]
fn float_accepts_separators_in_its_integer_part() {
    assert_eq!(number("1_000.5;"), TokenKind::Float(1000.5));
    assert_eq!(number("1.5;"), TokenKind::Float(1.5));
}

/// Verifies an integer literal too large for `i64` becomes a float instead of a refusal.
///
/// PHP 8.5.6 promotes on overflow: `echo 9223372036854775807;` prints it unchanged but
/// `echo 9223372036854775808;` prints `9.2233720368548E+18`, and the same holds for
/// `0x8000000000000000`. The eval lexer returned `InvalidNumber` for the decimal form.
#[test]
fn integer_overflow_promotes_to_float() {
    assert_eq!(
        number("9223372036854775807;"),
        TokenKind::Int(9_223_372_036_854_775_807)
    );
    assert_eq!(
        number("9223372036854775808;"),
        TokenKind::Float(9_223_372_036_854_775_808.0)
    );
    assert_eq!(
        number("0x8000000000000000;"),
        TokenKind::Float(9_223_372_036_854_775_808.0)
    );
}

/// Verifies the largest octal literal that still fits `i64` stays an integer.
/// PHP 8.5.6: `echo 0777777777777777777777;` prints `9223372036854775807`.
#[test]
fn largest_fitting_octal_literal_stays_an_integer() {
    assert_eq!(
        number("0777777777777777777777;"),
        TokenKind::Int(9_223_372_036_854_775_807)
    );
}

/// Verifies a digit outside the legacy octal range is refused, not silently reinterpreted.
///
/// PHP 8.5.6 refuses both at parse time: `Parse error: Invalid numeric literal`. Reading
/// them as decimal would be the same class of silent corruption as `0700` → `700`.
#[test]
fn out_of_range_legacy_octal_digit_is_refused() {
    assert_eq!(error("08;"), EvalParseError::InvalidNumber);
    assert_eq!(error("09;"), EvalParseError::InvalidNumber);
}

/// Verifies a radix prefix with no digits after it is refused rather than lexed as `0`
/// plus an identifier.
///
/// PHP 8.5.6 reaches the same refusal by a different route: its lexer splits `0x` into
/// `0` and the identifier `x`, and the PARSER then reports
/// `syntax error, unexpected identifier "x"`. elephc refuses in the lexer, matching the
/// AOT lexer, so what is pinned here is that the fragment is REFUSED — never that `0x`
/// quietly evaluates to `0`.
#[test]
fn radix_prefix_without_digits_is_refused() {
    assert_eq!(error("0x;"), EvalParseError::InvalidNumber);
    assert_eq!(error("0b;"), EvalParseError::InvalidNumber);
    assert_eq!(error("0o;"), EvalParseError::InvalidNumber);
}

/// Verifies a digit outside the prefix's base is refused instead of silently truncating
/// the literal at the first out-of-range digit.
///
/// PHP 8.5.6 refuses all three, again from the parser after a token split:
/// `0b12` → `syntax error, unexpected integer "2"`, `0o78` → `unexpected integer "8"`,
/// `0xfg` → `unexpected identifier "g"`. The silent outcome this forbids is `0b12`
/// evaluating to `1`.
#[test]
fn digit_outside_the_prefix_base_is_refused() {
    assert_eq!(error("0b12;"), EvalParseError::InvalidNumber);
    assert_eq!(error("0o78;"), EvalParseError::InvalidNumber);
    assert_eq!(error("0xfg;"), EvalParseError::InvalidNumber);
}

/// Verifies a dangling or doubled separator is refused, matching the AOT lexer's rule
/// that `_` is only legal BETWEEN digits.
///
/// PHP 8.5.6: `1_` → `syntax error, unexpected identifier "_"`, `1__0` → `unexpected
/// identifier "__0"`. Both are refusals, so accepting them as `1` would be a silent
/// divergence.
#[test]
fn misplaced_separator_is_refused() {
    assert_eq!(error("1_;"), EvalParseError::InvalidNumber);
    assert_eq!(error("1__0;"), EvalParseError::InvalidNumber);
}

/// Verifies a numeric literal butted straight against a PHP reserved word ends at the
/// keyword instead of being refused, so `eval('return 1and 2;')` means what it means in
/// PHP rather than raising a parse error.
///
/// Measured with `php -n` 8.5.6:
///
/// ```text
/// $ php -n -r 'var_dump(1and 2);'                => bool(true)
/// $ php -n -r 'var_dump(1or 2);'                 => bool(true)
/// $ php -n -r 'var_dump(1xor 2);'                => bool(false)
/// $ php -n -r 'var_dump(1instanceof stdClass);'  => bool(false)
/// $ php -n -r 'var_dump(1.5and 2);'              => bool(true)
/// $ php -n -r 'var_dump(1e2and 2);'              => bool(true)
/// $ php -n -r 'var_dump(0b11and 2);'             => bool(true)
/// $ php -n -r 'var_dump(1_000and 2);'            => bool(true)
/// ```
///
/// Before this test the eval lexer answered `InvalidNumber` for every one of them, in
/// lockstep with the AOT lexer — the two agreed with each other and diverged from PHP.
#[test]
fn numeric_literal_ends_at_a_reserved_word() {
    let ident = |name: &str| TokenKind::Ident(name.to_string());

    assert_eq!(
        kinds("1and 2;")[..3],
        [TokenKind::Int(1), ident("and"), TokenKind::Int(2)]
    );
    assert_eq!(
        kinds("1or 2;")[..3],
        [TokenKind::Int(1), ident("or"), TokenKind::Int(2)]
    );
    assert_eq!(
        kinds("1xor 2;")[..3],
        [TokenKind::Int(1), ident("xor"), TokenKind::Int(2)]
    );
    assert_eq!(
        kinds("1instanceof stdClass;")[..3],
        [TokenKind::Int(1), ident("instanceof"), ident("stdClass")]
    );
    assert_eq!(
        kinds("1.5and 2;")[..3],
        [TokenKind::Float(1.5), ident("and"), TokenKind::Int(2)]
    );
    assert_eq!(
        kinds("1e2and 2;")[..3],
        [TokenKind::Float(100.0), ident("and"), TokenKind::Int(2)]
    );
    assert_eq!(
        kinds("0b11and 2;")[..3],
        [TokenKind::Int(3), ident("and"), TokenKind::Int(2)]
    );
    assert_eq!(
        kinds("1_000and 2;")[..3],
        [TokenKind::Int(1000), ident("and"), TokenKind::Int(2)]
    );
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

/// Verifies the double-quoted escape table is unchanged by the interpolation rewrite.
///
/// `\x`, octal and `\u{}` escapes are still unsupported in this lexer (a separate,
/// declared debt), so they keep their backslash exactly as before.
#[test]
fn double_quoted_escape_table_is_unchanged() {
    assert_eq!(
        kinds(r#""a\nb\tc\\d\"e\q\x41";"#),
        vec![
            string("a\nb\tc\\d\"e\\q\\x41"),
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
