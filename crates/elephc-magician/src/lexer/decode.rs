//! Purpose:
//! Decodes the scalar values and operator shapes used by EvalIR.
//!
//! Called from:
//! - The projection of Chopper's runtime tokens.
//!
//! Key details:
//! - String and number boundaries are discovered by Chopper.
//! - The cursor decodes one literal and its interpolation values.

use elephc_chopper::scanner::{operator, Operator};

use super::{Token, TokenKind};
use crate::errors::EvalParseError;
use crate::eval_ir::EvalMagicConst;

/// Converts a scanned numeric literal, rejecting a following identifier or digit.
pub(super) fn decode_number(
    text: &str,
    after: &str,
    profile: elephc_semantics::SemanticProfile,
) -> Result<TokenKind, EvalParseError> {
    if after.chars().next().is_some_and(|ch| ch.is_alphanumeric() || ch == '_') {
        return Err(EvalParseError::InvalidNumber);
    }
    let digits = text.replace('_', "");
    let radix = match digits.as_bytes() {
        [b'0', b'x' | b'X', ..] => Some(16),
        [b'0', b'o' | b'O', ..] => Some(8),
        [b'0', b'b' | b'B', ..] => Some(2),
        _ => None,
    };
    if let Some(radix) = radix {
        return integer_token(&digits[2..], radix, profile);
    }
    if !digits.contains(['.', 'e', 'E']) {
        if digits.len() > 1 && digits.starts_with('0') {
            let octal = &digits[1..];
            if !octal.bytes().all(|digit| matches!(digit, b'0'..=b'7')) {
                return Err(EvalParseError::InvalidNumber);
            }
            return integer_token(octal, 8, profile);
        }
        return integer_token(&digits, 10, profile);
    }
    digits
        .parse::<f64>()
        .map(TokenKind::Float)
        .map_err(|_| EvalParseError::InvalidNumber)
}

/// Decodes one double-quoted literal and returns its extent with the eval tokens.
pub(super) fn decode_double_quoted(
    text: &str,
    line: i64,
    profile: elephc_semantics::SemanticProfile,
) -> Result<(Vec<Token>, usize), EvalParseError> {
    let mut lexer = LiteralCursor::new(text, profile);
    let tokens = lexer.lex_double_quoted(line)?;
    Ok((tokens, lexer.pos))
}

/// A cursor over the bytes of a scanned literal.
pub(super) struct LiteralCursor<'a> {
    source: &'a str,
    pos: usize,
    profile: elephc_semantics::SemanticProfile,
}

impl<'a> LiteralCursor<'a> {
    /// Creates a cursor at the start of the literal.
    fn new(source: &'a str, profile: elephc_semantics::SemanticProfile) -> Self {
        Self {
            source,
            pos: 0,
            profile,
        }
    }

    /// Returns the immutable semantic policy inherited by nested interpolation.
    pub(super) fn semantic_profile(&self) -> elephc_semantics::SemanticProfile {
        self.profile
    }

    /// Reads a PHP identifier body at the current byte offset.
    pub(super) fn lex_ident(&mut self) -> String {
        let mut ident = String::new();
        while let Some(ch) = self.peek_char() {
            if !is_ident_continue(ch) {
                break;
            }
            ident.push(ch);
            self.bump_char();
        }
        ident
    }

    /// Returns the source tail at the current character boundary.
    pub(super) fn remaining(&self) -> &'a str {
        &self.source[self.pos..]
    }

    /// Consumes a known byte range within the literal.
    pub(super) fn skip_bytes(&mut self, len: usize) {
        self.pos += len;
    }

    /// Returns the current char without advancing.
    pub(super) fn peek_char(&self) -> Option<char> {
        self.source[self.pos..].chars().next()
    }

    /// Returns the char after the current char without advancing.
    pub(super) fn peek_next_char(&self) -> Option<char> {
        let mut chars = self.source[self.pos..].chars();
        chars.next()?;
        chars.next()
    }

    /// Returns the char `offset` positions ahead of the cursor without advancing.
    ///
    /// `offset` 0 is the current char, so this generalizes `peek_char`/`peek_next_char`
    /// for the three-character lookahead that `"$obj->prop"` interpolation needs.
    pub(super) fn peek_nth_char(&self, offset: usize) -> Option<char> {
        self.source[self.pos..].chars().nth(offset)
    }

    /// Advances by one UTF-8 char.
    pub(super) fn bump_char(&mut self) {
        if let Some(ch) = self.peek_char() {
            self.pos += ch.len_utf8();
        }
    }
}

/// Adapts Chopper's operator boundaries to the token shapes consumed by the eval parser.
pub(super) fn eval_operator(source: &[u8]) -> Option<(TokenKind, usize)> {
    let (op, len) = operator(source)?;
    let kind = match op {
        // Preserve the eval lexer's two-token spelling for coalescing assignment.
        Operator::QuestionQuestionAssign => return Some((TokenKind::QuestionQuestion, 2)),
        // These spellings were separate punctuation in the eval token stream.
        Operator::LessGreater | Operator::PipeArrow => return None,
        Operator::StarStarAssign => TokenKind::StarStarEqual,
        Operator::Ellipsis => TokenKind::Ellipsis,
        Operator::Spaceship => TokenKind::Spaceship,
        Operator::EqualEqualEqual => TokenKind::EqualEqualEqual,
        Operator::NotEqualEqual => TokenKind::NotEqualEqual,
        Operator::LessLessAssign => TokenKind::LessLessEqual,
        Operator::GreaterGreaterAssign => TokenKind::GreaterGreaterEqual,
        Operator::QuestionArrow => TokenKind::QuestionArrow,
        Operator::StarStar => TokenKind::StarStar,
        Operator::PlusPlus => TokenKind::PlusPlus,
        Operator::MinusMinus => TokenKind::MinusMinus,
        Operator::DoubleArrow => TokenKind::FatArrow,
        Operator::LessEqual => TokenKind::LessEqual,
        Operator::GreaterEqual => TokenKind::GreaterEqual,
        Operator::EqualEqual => TokenKind::EqualEqual,
        Operator::NotEqual => TokenKind::NotEqual,
        Operator::PlusAssign => TokenKind::PlusEqual,
        Operator::MinusAssign => TokenKind::MinusEqual,
        Operator::StarAssign => TokenKind::StarEqual,
        Operator::SlashAssign => TokenKind::SlashEqual,
        Operator::DotAssign => TokenKind::DotEqual,
        Operator::PercentAssign => TokenKind::PercentEqual,
        Operator::AmpAssign => TokenKind::AmpEqual,
        Operator::PipeAssign => TokenKind::PipeEqual,
        Operator::CaretAssign => TokenKind::CaretEqual,
        Operator::AndAnd => TokenKind::AndAnd,
        Operator::OrOr => TokenKind::OrOr,
        Operator::QuestionQuestion => TokenKind::QuestionQuestion,
        Operator::DoubleColon => TokenKind::DoubleColon,
        Operator::Arrow => TokenKind::Arrow,
        Operator::LessLess => TokenKind::LessLess,
        Operator::GreaterGreater => TokenKind::GreaterGreater,
    };
    Some((kind, len))
}

/// Returns true for the first character of a PHP variable/function identifier.
pub(super) fn is_ident_start(ch: char) -> bool {
    ch == '_' || ch.is_ascii_alphabetic()
}

/// Returns true for subsequent characters in a PHP variable/function identifier.
fn is_ident_continue(ch: char) -> bool {
    is_ident_start(ch) || ch.is_ascii_digit()
}

/// Converts a PHP magic-constant identifier into a parser token when recognized.
pub(super) fn magic_const_token(name: &str, line: i64) -> Option<TokenKind> {
    let magic = if ident_eq(name, "__FILE__") {
        EvalMagicConst::File
    } else if ident_eq(name, "__DIR__") {
        EvalMagicConst::Dir
    } else if ident_eq(name, "__LINE__") {
        EvalMagicConst::Line(line)
    } else if ident_eq(name, "__FUNCTION__") {
        EvalMagicConst::Function
    } else if ident_eq(name, "__CLASS__") {
        EvalMagicConst::Class
    } else if ident_eq(name, "__METHOD__") {
        EvalMagicConst::Method
    } else if ident_eq(name, "__NAMESPACE__") {
        EvalMagicConst::Namespace
    } else if ident_eq(name, "__TRAIT__") {
        EvalMagicConst::Trait
    } else {
        return None;
    };
    Some(TokenKind::Magic(magic))
}

/// Compares a source identifier to a PHP keyword using ASCII case-insensitive rules.
fn ident_eq(actual: &str, expected: &str) -> bool {
    actual.eq_ignore_ascii_case(expected)
}

/// Converts integer digits to an integer, or to a float when they overflow `i64`.
///
/// PHP parses over-large decimal literals through its decimal float conversion. Other bases
/// mirror the compiler's `radix_digits_to_float` accumulation.
fn eval_radix_int_or_float(digits: &str, radix: u32) -> TokenKind {
    if let Ok(value) = i64::from_str_radix(digits, radix) {
        return TokenKind::Int(value);
    }
    if radix == 10 {
        return TokenKind::Float(
            digits
                .parse::<f64>()
                .expect("scanner only passes valid decimal digits"),
        );
    }
    let radix_float = f64::from(radix);
    let mut value = 0.0_f64;
    for ch in digits.chars() {
        let digit = ch
            .to_digit(radix)
            .expect("scanner only passes digits valid for this radix");
        value = value * radix_float + f64::from(digit);
    }
    TokenKind::Float(value)
}

/// Keeps checked integer magnitude separate from PHP's default float promotion.
fn integer_token(
    digits: &str,
    radix: u32,
    profile: elephc_semantics::SemanticProfile,
) -> Result<TokenKind, EvalParseError> {
    use elephc_semantics::integer_literals::{classify_integer_digits, IntegerMagnitude};
    use elephc_semantics::StrictFeature;

    if profile.contains(StrictFeature::Overflow) {
        return match classify_integer_digits(digits, radix) {
            IntegerMagnitude::Signed(value) => Ok(TokenKind::Int(value)),
            IntegerMagnitude::MinimumMagnitude => Ok(TokenKind::MinimumIntegerMagnitude),
            IntegerMagnitude::Overflow => Err(EvalParseError::IntegerLiteralOverflow),
            IntegerMagnitude::Invalid => Err(EvalParseError::InvalidNumber),
        };
    }
    Ok(eval_radix_int_or_float(digits, radix))
}
