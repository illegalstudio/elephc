//! Purpose:
//! Converts scanned numbers to compiler literal tokens.
//! Handles radix prefixes, underscores, decimal/exponent floats, and dot-started floats.
//!
//! Called from:
//! - `crate::syntax::lexer::scan` through `crate::syntax::lexer::literals`.
//!
//! Key details:
//! - Invalid trailing alphanumerics become lexer errors instead of being split into misleading tokens.

use crate::syntax::parser::validation_only;
use super::super::cursor::Cursor;
use super::super::token::Token;
use crate::syntax::errors::CompileError;
use crate::scanner::NumberKind;
use crate::syntax::span::Span;
use elephc_semantics::{
    integer_literals::{classify_integer_digits, IntegerMagnitude},
    StrictFeature,
};
use std::num::IntErrorKind;

/// After scanning a numeric literal, ensure no alphanumeric character or `_`
/// follows. Catches malformed forms like `0o78`, `078`, `0xfg`, `0b12`, `1_`,
/// and `1__0`, which PHP rejects at parse time but the lexer would otherwise
/// silently split into two adjacent tokens.
#[allow(unreachable_code)]
fn validate_no_trailing_alnum(cursor: &Cursor, base_label: &str) -> Result<(), CompileError> {
    // PHP's lexer ends the number at the last valid digit and lets the remaining bytes
    // lex as a label (`1_` is T_LNUMBER + T_STRING). Only the compiler keeps this as an
    // error so its own numeric diagnostics stay precise.
    if validation_only() {
        return Ok(());
    }
    if let Some(ch) = cursor.peek() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            return Err(CompileError::new(
                cursor.span(),
                &format!("Unexpected character '{ch}' after {base_label} literal"),
            ));
        }
    }
    Ok(())
}

/// Converts a digit string in `digits` (all valid for `radix`) to an f64 by
/// accumulating `value = value * radix + digit` for each character.
/// Panics if `digits` contains any character not valid for `radix`.
fn radix_digits_to_float(digits: &str, radix: u32) -> f64 {
    let radix_float = radix as f64;
    let mut value = 0.0;
    for ch in digits.chars() {
        let digit = ch
            .to_digit(radix)
            .expect("scanner should only pass valid radix digits") as f64;
        value = value * radix_float + digit;
    }
    value
}

/// Preserves the minimum signed magnitude and rejects larger integer spellings before rounding.
fn strict_integer_token(
    cursor: &Cursor,
    digits: &str,
    radix: u32,
    span: Span,
) -> Result<Option<Token>, CompileError> {
    if !cursor.semantic_profile.contains(StrictFeature::Overflow) {
        return Ok(None);
    }
    match classify_integer_digits(digits, radix) {
        IntegerMagnitude::Signed(value) => Ok(Some(Token::IntLiteral(value))),
        IntegerMagnitude::MinimumMagnitude => Ok(Some(Token::MinimumIntegerMagnitude)),
        IntegerMagnitude::Overflow => Err(CompileError::semantic_violation(
            span,
            StrictFeature::Overflow,
            "--strict-overflow: integer literal is outside signed 64-bit range",
        )),
        IntegerMagnitude::Invalid => Err(CompileError::new(span, "Invalid integer literal")),
    }
}

/// Parses a radix-prefixed digit string as either an `i64` integer or an `f64`
/// float (on positive overflow). Returns `Token::IntLiteral` or `Token::FloatLiteral`.
/// On a parse error unrelated to overflow, returns the provided error message.
fn parse_radix_int_or_float(
    cursor: &Cursor,
    digits: &str,
    radix: u32,
    invalid_message: &str,
    span: Span,
) -> Result<Token, CompileError> {
    if let Some(token) = strict_integer_token(cursor, digits, radix, span)? {
        return Ok(token);
    }
    match i64::from_str_radix(digits, radix) {
        Ok(value) => Ok(Token::IntLiteral(value)),
        Err(err) if matches!(err.kind(), IntErrorKind::PosOverflow) => {
            Ok(Token::FloatLiteral(radix_digits_to_float(digits, radix)))
        }
        Err(_) => Err(CompileError::new(cursor.span(), invalid_message)),
    }
}

/// Parses a decimal digit string as either an `i64` integer or an `f64` float
/// (on positive overflow). Returns `Token::IntLiteral` or `Token::FloatLiteral`.
/// On any other parse error returns "Invalid integer literal".
fn parse_decimal_int_or_float(
    cursor: &Cursor,
    digits: &str,
    span: Span,
) -> Result<Token, CompileError> {
    if let Some(token) = strict_integer_token(cursor, digits, 10, span)? {
        return Ok(token);
    }
    match digits.parse::<i64>() {
        Ok(value) => Ok(Token::IntLiteral(value)),
        Err(err) if matches!(err.kind(), IntErrorKind::PosOverflow) => {
            let value = digits
                .parse::<f64>()
                .map_err(|_| CompileError::new(cursor.span(), "Invalid integer literal"))?;
            Ok(Token::FloatLiteral(value))
        }
        Err(_) => Err(CompileError::new(cursor.span(), "Invalid integer literal")),
    }
}

/// Converts a numeric lexeme and retains the compiler's trailing-input diagnostics.
pub(in crate::syntax::lexer) fn number_token(
    text: &str,
    kind: NumberKind,
    start: Span,
    end: &Cursor,
) -> Result<Token, CompileError> {
    match kind {
        NumberKind::Prefixed(radix) => {
            let (label, missing, invalid) = match radix {
                16 => ("hex", "Expected hex digits after '0x'", "Invalid hex literal"),
                8 => ("octal", "Expected octal digits after '0o'", "Invalid octal literal"),
                _ => ("binary", "Expected binary digits after '0b'", "Invalid binary literal"),
            };
            if text.len() == 2 {
                return Err(CompileError::new(start, missing));
            }
            validate_no_trailing_alnum(end, label)?;
            parse_radix_int_or_float(end, &text[2..].replace('_', ""), radix, invalid, start)
        }
        NumberKind::Float => {
            validate_no_trailing_alnum(end, "float")?;
            text.replace('_', "")
                .parse::<f64>()
                .map(Token::FloatLiteral)
                .map_err(|_| CompileError::new(end.span(), "Invalid float literal"))
        }
        NumberKind::LegacyOctal => {
            validate_no_trailing_alnum(end, "octal")?;
            let message = if validation_only() { "Invalid numeric literal" } else { "Invalid octal literal" };
            parse_radix_int_or_float(end, &text.replace('_', ""), 8, message, start)
        }
        NumberKind::Decimal => {
            validate_no_trailing_alnum(end, "decimal")?;
            parse_decimal_int_or_float(end, &text.replace('_', ""), start)
        }
    }
}
