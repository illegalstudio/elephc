//! Purpose:
//! Scans integer and floating-point literals across PHP-supported numeric forms.
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
use crate::scanner::digits_len;
use std::num::IntErrorKind;

/// Collects one digit run with separators removed for numeric conversion.
fn scan_radix_digits(cursor: &mut Cursor, radix: u32) -> String {
    let remaining = cursor.remaining();
    let len = digits_len(remaining.as_bytes(), radix);
    cursor.skip_ascii(len);
    remaining[..len].replace('_', "")
}

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

/// Returns whether the bytes after an `e`/`E` contain an optional sign and at least one digit.
fn exponent_has_digits(remaining: &str) -> bool {
    let bytes = remaining.as_bytes();
    let mut index = 1; // skip e/E
    if matches!(bytes.get(index), Some(b'+') | Some(b'-')) {
        index += 1;
    }
    bytes.get(index).is_some_and(u8::is_ascii_digit)
}

/// Parses a radix-prefixed digit string as either an `i64` integer or an `f64`
/// float (on positive overflow). Returns `Token::IntLiteral` or `Token::FloatLiteral`.
/// On a parse error unrelated to overflow, returns the provided error message.
fn parse_radix_int_or_float(
    cursor: &Cursor,
    digits: &str,
    radix: u32,
    invalid_message: &str,
) -> Result<Token, CompileError> {
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
fn parse_decimal_int_or_float(cursor: &Cursor, digits: &str) -> Result<Token, CompileError> {
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

/// Scans a numeric literal from the cursor: hex (`0x`), octal (`0o`), binary (`0b`),
/// decimal, or float (including scientific notation). Validates no trailing
/// alphanumeric characters remain. Returns `Token::IntLiteral` or `Token::FloatLiteral`.
/// Handles PHP 7.4+ underscore numeric separators and legacy octal forms.
pub(in crate::syntax::lexer) fn scan_number(cursor: &mut Cursor) -> Result<Token, CompileError> {
    // Captured before consuming any prefix so empty-literal errors point at the literal start.
    let start_span = cursor.span();
    if cursor.peek() == Some('0') {
        let remaining = cursor.remaining();
        if remaining.len() > 1 {
            let prefix = remaining.as_bytes()[1];

            let validation = validation_only();
            if (prefix == b'x' || prefix == b'X')
                && (!validation
                    || remaining
                        .as_bytes()
                        .get(2)
                        .is_some_and(|byte| byte.is_ascii_hexdigit()))
            {
                cursor.advance();
                cursor.advance();
                let hex_str = scan_radix_digits(cursor, 16);
                if hex_str.is_empty() {
                    return Err(CompileError::new(
                        start_span,
                        "Expected hex digits after '0x'",
                    ));
                }
                validate_no_trailing_alnum(cursor, "hex")?;
                return parse_radix_int_or_float(cursor, &hex_str, 16, "Invalid hex literal");
            }

            if (prefix == b'o' || prefix == b'O')
                && (!validation
                    || remaining
                        .as_bytes()
                        .get(2)
                        .is_some_and(|byte| matches!(byte, b'0'..=b'7')))
            {
                cursor.advance();
                cursor.advance();
                let octal_str = scan_radix_digits(cursor, 8);
                if octal_str.is_empty() {
                    return Err(CompileError::new(
                        start_span,
                        "Expected octal digits after '0o'",
                    ));
                }
                validate_no_trailing_alnum(cursor, "octal")?;
                return parse_radix_int_or_float(cursor, &octal_str, 8, "Invalid octal literal");
            }

            if (prefix == b'b' || prefix == b'B')
                && (!validation
                    || remaining
                        .as_bytes()
                        .get(2)
                        .is_some_and(|byte| matches!(byte, b'0' | b'1')))
            {
                cursor.advance();
                cursor.advance();
                let bin_str = scan_radix_digits(cursor, 2);
                if bin_str.is_empty() {
                    return Err(CompileError::new(
                        start_span,
                        "Expected binary digits after '0b'",
                    ));
                }
                validate_no_trailing_alnum(cursor, "binary")?;
                return parse_radix_int_or_float(cursor, &bin_str, 2, "Invalid binary literal");
            }
        }
    }

    let mut num_str = scan_radix_digits(cursor, 10);

    let is_float = if cursor.peek() == Some('.') {
        let remaining = cursor.remaining();
        remaining.len() > 1 && (remaining.as_bytes()[1] as char).is_ascii_digit()
    } else {
        false
    };

    let is_sci = matches!(cursor.peek(), Some('e') | Some('E'))
        && (exponent_has_digits(cursor.remaining()) || !validation_only());

    if is_float || is_sci {
        if is_float {
            num_str.push('.');
            cursor.advance();
            num_str.push_str(&scan_radix_digits(cursor, 10));
        }
        if matches!(cursor.peek(), Some('e') | Some('E')) {
            num_str.push('e');
            cursor.advance();
            if let Some(sign @ ('+' | '-')) = cursor.peek() {
                num_str.push(sign);
                cursor.advance();
            }
            num_str.push_str(&scan_radix_digits(cursor, 10));
        }
        validate_no_trailing_alnum(cursor, "float")?;
        let value: f64 = num_str
            .parse()
            .map_err(|_| CompileError::new(cursor.span(), "Invalid float literal"))?;
        return Ok(Token::FloatLiteral(value));
    }

    let is_legacy_octal = num_str.len() > 1 && num_str.starts_with('0');
    validate_no_trailing_alnum(
        cursor,
        if is_legacy_octal { "octal" } else { "decimal" },
    )?;
    if is_legacy_octal {
        let message = if validation_only() {
            "Invalid numeric literal"
        } else {
            "Invalid octal literal"
        };
        return parse_radix_int_or_float(cursor, &num_str, 8, message);
    }

    parse_decimal_int_or_float(cursor, &num_str)
}

/// Scan a float literal starting with `.` (e.g., `.5`, `.123`)
pub(in crate::syntax::lexer) fn scan_dot_float(cursor: &mut Cursor) -> Result<Token, CompileError> {
    let mut num_str = String::from("0.");
    cursor.advance();

    num_str.push_str(&scan_radix_digits(cursor, 10));

    if matches!(cursor.peek(), Some('e') | Some('E'))
        && (exponent_has_digits(cursor.remaining()) || !validation_only())
    {
        num_str.push('e');
        cursor.advance();
        if let Some(sign @ ('+' | '-')) = cursor.peek() {
            num_str.push(sign);
            cursor.advance();
        }
        num_str.push_str(&scan_radix_digits(cursor, 10));
    }

    validate_no_trailing_alnum(cursor, "float")?;

    let value: f64 = num_str
        .parse()
        .map_err(|_| CompileError::new(cursor.span(), "Invalid float literal"))?;

    Ok(Token::FloatLiteral(value))
}
