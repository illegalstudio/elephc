//! Purpose:
//! Scans UTF-8 eval source fragments into eval parser tokens.
//! This file owns trivia skipping, literal lexing, PHP string escapes, and
//! magic-constant token recognition.
//!
//! Called from:
//! - `crate::lexer::tokenize()` re-exported by `crate::lexer`.
//!
//! Key details:
//! - Comments and whitespace advance line metadata for `__LINE__`.
//! - Unterminated strings or block comments return parse errors before grammar parsing.
//! - Double-quoted literals are expanded by `super::strings` into concatenation token
//!   streams, so one source character can yield more than one token.

use super::{Token, TokenKind};
use crate::errors::EvalParseError;
use crate::eval_ir::EvalMagicConst;

/// Tokenizes a complete source fragment and appends an EOF sentinel.
pub(crate) fn tokenize(source: &str) -> Result<Vec<Token>, EvalParseError> {
    Lexer::new(source).tokenize()
}

/// Converts a UTF-8 eval source fragment into parser tokens.
pub(super) struct Lexer<'a> {
    source: &'a str,
    pos: usize,
    line: i64,
}

impl<'a> Lexer<'a> {
    /// Creates a lexer over a UTF-8 eval fragment.
    fn new(source: &'a str) -> Self {
        Self {
            source,
            pos: 0,
            line: 1,
        }
    }

    /// Tokenizes the complete source and appends an EOF sentinel.
    fn tokenize(mut self) -> Result<Vec<Token>, EvalParseError> {
        let mut tokens = Vec::new();
        loop {
            let batch = self.next_tokens()?;
            let done = batch
                .last()
                .is_some_and(|token| *token.kind() == TokenKind::Eof);
            tokens.extend(batch);
            if done {
                break;
            }
        }
        Ok(tokens)
    }

    /// Reads the next token batch from the source.
    ///
    /// Every source construct except a double-quoted literal yields exactly one token.
    /// A double-quoted literal yields one `TokenKind::String` when it contains no
    /// interpolation, and a parenthesized concatenation token stream when it does.
    fn next_tokens(&mut self) -> Result<Vec<Token>, EvalParseError> {
        self.skip_trivia()?;
        let Some(ch) = self.peek_char() else {
            return Ok(vec![Token::new(TokenKind::Eof, self.line)]);
        };
        let line = self.line;
        if ch == '"' {
            return self.lex_double_quoted(line);
        }
        let kind = match ch {
            '$' => self.lex_variable(),
            '\'' => self.lex_single_quoted(),
            '0'..='9' => self.lex_number(),
            '+' => {
                self.bump_char();
                if self.peek_char() == Some('+') {
                    self.bump_char();
                    Ok(TokenKind::PlusPlus)
                } else if self.peek_char() == Some('=') {
                    self.bump_char();
                    Ok(TokenKind::PlusEqual)
                } else {
                    Ok(TokenKind::Plus)
                }
            }
            '-' => {
                self.bump_char();
                if self.peek_char() == Some('>') {
                    self.bump_char();
                    Ok(TokenKind::Arrow)
                } else if self.peek_char() == Some('-') {
                    self.bump_char();
                    Ok(TokenKind::MinusMinus)
                } else if self.peek_char() == Some('=') {
                    self.bump_char();
                    Ok(TokenKind::MinusEqual)
                } else {
                    Ok(TokenKind::Minus)
                }
            }
            '*' => {
                self.bump_char();
                if self.peek_char() == Some('*') {
                    self.bump_char();
                    if self.peek_char() == Some('=') {
                        self.bump_char();
                        Ok(TokenKind::StarStarEqual)
                    } else {
                        Ok(TokenKind::StarStar)
                    }
                } else if self.peek_char() == Some('=') {
                    self.bump_char();
                    Ok(TokenKind::StarEqual)
                } else {
                    Ok(TokenKind::Star)
                }
            }
            '/' => {
                self.bump_char();
                if self.peek_char() == Some('=') {
                    self.bump_char();
                    Ok(TokenKind::SlashEqual)
                } else {
                    Ok(TokenKind::Slash)
                }
            }
            '%' => {
                self.bump_char();
                if self.peek_char() == Some('=') {
                    self.bump_char();
                    Ok(TokenKind::PercentEqual)
                } else {
                    Ok(TokenKind::Percent)
                }
            }
            '.' => {
                self.bump_char();
                if self.peek_char() == Some('.') && self.peek_next_char() == Some('.') {
                    self.bump_char();
                    self.bump_char();
                    Ok(TokenKind::Ellipsis)
                } else if self.peek_char() == Some('=') {
                    self.bump_char();
                    Ok(TokenKind::DotEqual)
                } else {
                    Ok(TokenKind::Dot)
                }
            }
            '=' => {
                self.bump_char();
                if self.peek_char() == Some('=') {
                    self.bump_char();
                    if self.peek_char() == Some('=') {
                        self.bump_char();
                        Ok(TokenKind::EqualEqualEqual)
                    } else {
                        Ok(TokenKind::EqualEqual)
                    }
                } else if self.peek_char() == Some('>') {
                    self.bump_char();
                    Ok(TokenKind::FatArrow)
                } else {
                    Ok(TokenKind::Equal)
                }
            }
            '!' => {
                self.bump_char();
                if self.peek_char() == Some('=') {
                    self.bump_char();
                    if self.peek_char() == Some('=') {
                        self.bump_char();
                        Ok(TokenKind::NotEqualEqual)
                    } else {
                        Ok(TokenKind::NotEqual)
                    }
                } else {
                    Ok(TokenKind::Bang)
                }
            }
            '&' => {
                self.bump_char();
                if self.peek_char() == Some('&') {
                    self.bump_char();
                    Ok(TokenKind::AndAnd)
                } else if self.peek_char() == Some('=') {
                    self.bump_char();
                    Ok(TokenKind::AmpEqual)
                } else {
                    Ok(TokenKind::Ampersand)
                }
            }
            '|' => {
                self.bump_char();
                if self.peek_char() == Some('|') {
                    self.bump_char();
                    Ok(TokenKind::OrOr)
                } else if self.peek_char() == Some('=') {
                    self.bump_char();
                    Ok(TokenKind::PipeEqual)
                } else {
                    Ok(TokenKind::Pipe)
                }
            }
            '^' => {
                self.bump_char();
                if self.peek_char() == Some('=') {
                    self.bump_char();
                    Ok(TokenKind::CaretEqual)
                } else {
                    Ok(TokenKind::Caret)
                }
            }
            '~' => {
                self.bump_char();
                Ok(TokenKind::Tilde)
            }
            '<' => {
                self.bump_char();
                if self.peek_char() == Some('<') {
                    self.bump_char();
                    if self.peek_char() == Some('=') {
                        self.bump_char();
                        Ok(TokenKind::LessLessEqual)
                    } else {
                        Ok(TokenKind::LessLess)
                    }
                } else if self.peek_char() == Some('=') {
                    self.bump_char();
                    if self.peek_char() == Some('>') {
                        self.bump_char();
                        Ok(TokenKind::Spaceship)
                    } else {
                        Ok(TokenKind::LessEqual)
                    }
                } else {
                    Ok(TokenKind::Less)
                }
            }
            '>' => {
                self.bump_char();
                if self.peek_char() == Some('>') {
                    self.bump_char();
                    if self.peek_char() == Some('=') {
                        self.bump_char();
                        Ok(TokenKind::GreaterGreaterEqual)
                    } else {
                        Ok(TokenKind::GreaterGreater)
                    }
                } else if self.peek_char() == Some('=') {
                    self.bump_char();
                    Ok(TokenKind::GreaterEqual)
                } else {
                    Ok(TokenKind::Greater)
                }
            }
            '?' => {
                self.bump_char();
                if self.peek_char() == Some('-') && self.peek_next_char() == Some('>') {
                    self.bump_char();
                    self.bump_char();
                    Ok(TokenKind::QuestionArrow)
                } else if self.peek_char() == Some('?') {
                    self.bump_char();
                    Ok(TokenKind::QuestionQuestion)
                } else {
                    Ok(TokenKind::Question)
                }
            }
            ';' => {
                self.bump_char();
                Ok(TokenKind::Semicolon)
            }
            '(' => {
                self.bump_char();
                Ok(TokenKind::LParen)
            }
            ')' => {
                self.bump_char();
                Ok(TokenKind::RParen)
            }
            '[' => {
                self.bump_char();
                Ok(TokenKind::LBracket)
            }
            ']' => {
                self.bump_char();
                Ok(TokenKind::RBracket)
            }
            '{' => {
                self.bump_char();
                Ok(TokenKind::LBrace)
            }
            '}' => {
                self.bump_char();
                Ok(TokenKind::RBrace)
            }
            ',' => {
                self.bump_char();
                Ok(TokenKind::Comma)
            }
            ':' => {
                self.bump_char();
                if self.peek_char() == Some(':') {
                    self.bump_char();
                    Ok(TokenKind::DoubleColon)
                } else {
                    Ok(TokenKind::Colon)
                }
            }
            '\\' => {
                self.bump_char();
                Ok(TokenKind::Backslash)
            }
            '#' if self.peek_next_char() == Some('[') => {
                self.bump_char();
                self.bump_char();
                Ok(TokenKind::AttributeStart)
            }
            _ if is_ident_start(ch) => {
                let ident = self.lex_ident();
                Ok(magic_const_token(&ident, line).unwrap_or(TokenKind::Ident(ident)))
            }
            _ => Err(EvalParseError::UnexpectedToken),
        }?;
        Ok(vec![Token::new(kind, line)])
    }

    /// Reads a `$name` token.
    fn lex_variable(&mut self) -> Result<TokenKind, EvalParseError> {
        self.bump_char();
        if self.peek_char() == Some('{') {
            self.bump_char();
            return Ok(TokenKind::DollarLBrace);
        }
        let name = self.lex_ident();
        if name.is_empty() {
            return Err(EvalParseError::ExpectedVariable);
        }
        Ok(TokenKind::DollarIdent(name))
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
    fn lex_number(&mut self) -> Result<TokenKind, EvalParseError> {
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
            return Ok(eval_radix_int_or_float(octal, 8));
        }
        Ok(match digits.parse::<i64>() {
            Ok(value) => TokenKind::Int(value),
            // PHP promotes an integer literal past `PHP_INT_MAX` to a float rather than
            // rejecting it.
            Err(_) => TokenKind::Float(digits.parse::<f64>().map_err(|_| {
                EvalParseError::InvalidNumber
            })?),
        })
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
        Ok(eval_radix_int_or_float(&digits, radix))
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

    /// Reads a single-quoted string literal, which never interpolates.
    fn lex_single_quoted(&mut self) -> Result<TokenKind, EvalParseError> {
        self.bump_char();
        let mut out = String::new();
        while let Some(ch) = self.peek_char() {
            self.bump_char();
            if ch == '\'' {
                return Ok(TokenKind::String(out));
            }
            if ch == '\\' {
                let Some(escaped) = self.peek_char() else {
                    return Err(EvalParseError::UnterminatedString);
                };
                self.bump_char();
                match escaped {
                    '\\' => out.push('\\'),
                    '\'' => out.push('\''),
                    other => {
                        out.push('\\');
                        elephc_builtin_contract::string_literal::push_literal_char(other, &mut out);
                    }
                }
            } else {
                elephc_builtin_contract::string_literal::push_literal_char(ch, &mut out);
            }
        }
        Err(EvalParseError::UnterminatedString)
    }

    /// Advances past ASCII/Unicode whitespace and PHP comments.
    fn skip_trivia(&mut self) -> Result<(), EvalParseError> {
        loop {
            while self.peek_char().is_some_and(char::is_whitespace) {
                self.bump_char();
            }
            match (self.peek_char(), self.peek_next_char()) {
                (Some('/'), Some('/')) => self.skip_line_comment(),
                (Some('#'), Some('[')) => return Ok(()),
                (Some('#'), _) => self.skip_line_comment(),
                (Some('/'), Some('*')) => self.skip_block_comment()?,
                _ => return Ok(()),
            }
        }
    }

    /// Advances past a `//` or `#` comment, including its trailing newline when present.
    fn skip_line_comment(&mut self) {
        while let Some(ch) = self.peek_char() {
            self.bump_char();
            if ch == '\n' {
                break;
            }
        }
    }

    /// Advances past a `/* ... */` comment while preserving fragment line metadata.
    fn skip_block_comment(&mut self) -> Result<(), EvalParseError> {
        self.bump_char();
        self.bump_char();
        while let Some(ch) = self.peek_char() {
            if ch == '*' && self.peek_next_char() == Some('/') {
                self.bump_char();
                self.bump_char();
                return Ok(());
            }
            self.bump_char();
        }
        Err(EvalParseError::UnterminatedComment)
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
            if ch == '\n' {
                self.line += 1;
            }
        }
    }
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
fn magic_const_token(name: &str, line: i64) -> Option<TokenKind> {
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

/// Converts radix digits to an integer, or to a float when they overflow `i64`.
///
/// PHP promotes an over-large literal in any base to a float rather than rejecting it, so the
/// accumulation mirrors the compiler's `radix_digits_to_float`.
fn eval_radix_int_or_float(digits: &str, radix: u32) -> TokenKind {
    if let Ok(value) = i64::from_str_radix(digits, radix) {
        return TokenKind::Int(value);
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
