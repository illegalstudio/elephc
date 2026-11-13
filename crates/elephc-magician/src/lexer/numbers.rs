//! Purpose:
//! Scans eval numeric literals without losing checked integer spelling boundaries.
//!
//! Called from:
//! - The eval lexer token dispatcher.
//!
//! Key details:
//! - Compiler and eval share exact unsigned magnitude classification.

use super::scan::Lexer;
use super::TokenKind;
use crate::errors::EvalParseError;
use elephc_semantics::{SemanticProfile, StrictFeature};
use elephc_semantics::integer_literals::{classify_integer_digits, IntegerMagnitude};

impl Lexer<'_> {
    /// Reads an integer or float literal, in every base PHP writes one in.
    ///
    /// This used to read decimal digits only, which made `0700` the integer SEVEN HUNDRED
    /// instead of 448, and made `0x1F`, `0b101`, `0o17` and `1_000` reject the whole fragment
    /// as invalid. Both are reachable from a plain `eval('return mkdir($p, 0700, true);')`.
    ///
    /// The grammar mirrors the compiler's own `lexer::literals::numbers::scan_number`, which
    /// is the authority: `0x`/`0X` hex, `0b`/`0B` binary, `0o`/`0O` octal, legacy leading-zero
    /// octal, `_` separators anywhere between digits, and decimal with an optional fraction
    /// and exponent. A literal too large for `i64` becomes a float, as it does in PHP.
    pub(super) fn lex_number(&mut self) -> Result<TokenKind, EvalParseError> {
        if self.peek_char() == Some('0') {
            if let Some(prefix) = self.peek_next_char() {
                match prefix {
                    'x' | 'X' => return self.lex_radix_number(16, |c| c.is_ascii_hexdigit()),
                    'o' | 'O' => {
                        return self.lex_radix_number(8, |c| c.is_ascii_digit() && c < '8')
                    }
                    'b' | 'B' => return self.lex_radix_number(2, |c| c == '0' || c == '1'),
                    _ => {}
                }
            }
        }

        let mut digits = self.lex_digits(|c| c.is_ascii_digit());
        if digits.is_empty() {
            return Err(EvalParseError::InvalidNumber);
        }

        let has_fraction =
            self.peek_char() == Some('.') && matches!(self.peek_next_char(), Some('0'..='9'));
        let has_exponent = matches!(self.peek_char(), Some('e') | Some('E'));
        if has_fraction || has_exponent {
            if has_fraction {
                digits.push('.');
                self.bump_char();
                digits.push_str(&self.lex_digits(|c| c.is_ascii_digit()));
            }
            if matches!(self.peek_char(), Some('e') | Some('E')) {
                digits.push('e');
                self.bump_char();
                if let Some(sign @ ('+' | '-')) = self.peek_char() {
                    digits.push(sign);
                    self.bump_char();
                }
                digits.push_str(&self.lex_digits(|c| c.is_ascii_digit()));
            }
            self.reject_trailing_literal_char()?;
            return digits
                .parse::<f64>()
                .map(TokenKind::Float)
                .map_err(|_| EvalParseError::InvalidNumber);
        }
        self.reject_trailing_literal_char()?;

        // A leading zero followed by more digits is PHP's legacy octal spelling. `8` and `9`
        // are not octal digits, so `078` is a PHP parse error ("Invalid numeric literal") and
        // has to be REFUSED here — the conversion below would otherwise reach a `to_digit(8)`
        // that cannot answer, and panic the interpreter.
        if digits.len() > 1 && digits.starts_with('0') {
            let octal = &digits[1..];
            if !octal.chars().all(|ch| ('0'..'8').contains(&ch)) {
                return Err(EvalParseError::InvalidNumber);
            }
            return integer_token(octal, 8, self.semantic_profile());
        }
        integer_token(&digits, 10, self.semantic_profile())
    }

    /// Reads the digits of a `0x` / `0o` / `0b` literal, prefix included.
    fn lex_radix_number(
        &mut self,
        radix: u32,
        is_digit: impl Fn(char) -> bool,
    ) -> Result<TokenKind, EvalParseError> {
        self.bump_char();
        self.bump_char();
        let digits = self.lex_digits(is_digit);
        if digits.is_empty() {
            return Err(EvalParseError::InvalidNumber);
        }
        self.reject_trailing_literal_char()?;
        integer_token(&digits, radix, self.semantic_profile())
    }

    /// Refuses a literal that runs straight into another alphanumeric byte.
    ///
    /// `0o78`, `0x1G` and `123abc` are PHP parse errors. Without this the digit scanner would
    /// simply stop at the offending byte and hand the rest to the next token, turning a
    /// rejected literal into a silently different program. Mirrors the compiler's own
    /// `lexer::literals::numbers::validate_no_trailing_alnum`.
    fn reject_trailing_literal_char(&mut self) -> Result<(), EvalParseError> {
        match self.peek_char() {
            Some(ch) if ch.is_alphanumeric() || ch == '_' => Err(EvalParseError::InvalidNumber),
            _ => Ok(()),
        }
    }

    /// Reads a run of digits accepted by `is_digit`, dropping PHP's `_` separators.
    fn lex_digits(&mut self, is_digit: impl Fn(char) -> bool) -> String {
        let mut digits = String::new();
        while let Some(ch) = self.peek_char() {
            if ch == '_' {
                // A separator is only legal BETWEEN digits, which is what this checks.
                if digits.is_empty() || !self.peek_next_char().is_some_and(&is_digit) {
                    break;
                }
                self.bump_char();
                continue;
            }
            if !is_digit(ch) {
                break;
            }
            digits.push(ch);
            self.bump_char();
        }
        digits
    }

}

/// Keeps checked integer magnitude separate from PHP's default float promotion.
fn integer_token(digits: &str, radix: u32, profile: SemanticProfile) -> Result<TokenKind, EvalParseError> {
    if profile.contains(StrictFeature::Overflow) {
        return match classify_integer_digits(digits, radix) {
            IntegerMagnitude::Signed(value) => Ok(TokenKind::Int(value)),
            IntegerMagnitude::MinimumMagnitude => Ok(TokenKind::MinimumIntegerMagnitude),
            IntegerMagnitude::Overflow => Err(EvalParseError::IntegerLiteralOverflow),
            IntegerMagnitude::Invalid => Err(EvalParseError::InvalidNumber),
        };
    }
    if let Ok(value) = i64::from_str_radix(digits, radix) { return Ok(TokenKind::Int(value)); }
    if radix == 10 {
        return digits.parse::<f64>().map(TokenKind::Float).map_err(|_| EvalParseError::InvalidNumber);
    }
    let mut value = 0.0_f64;
    for character in digits.chars() {
        let digit = character.to_digit(radix).ok_or(EvalParseError::InvalidNumber)?;
        value = value * f64::from(radix) + f64::from(digit);
    }
    Ok(TokenKind::Float(value))
}
