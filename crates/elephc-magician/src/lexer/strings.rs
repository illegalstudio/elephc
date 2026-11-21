//! Purpose:
//! Expands double-quoted eval-fragment string literals into interpolation-aware token
//! streams. A literal without interpolation stays exactly one `TokenKind::String`; a
//! literal with interpolation becomes a parenthesized `.` concatenation the existing
//! grammar already lowers to `EvalBinOp::Concat`.
//!
//! Called from:
//! - The projection of Chopper's quoted string tokens.
//!
//! Key details:
//! - Escape handling covers PHP's simple, hexadecimal, and octal forms; `\$` still
//!   yields a literal `$` and never interpolates.
//! - Shared literal markers preserve escaped bytes and avoid collisions with private-use source text.
//! - Every synthetic token carries the line of the opening quote, keeping `__LINE__`
//!   stable across multi-line literals.
//! - PHP simple syntax allows exactly one `[offset]` or `->prop` after `$name`; anything
//!   deeper requires the complex `{$expr}` form.
//! - Malformed offsets (`"$a[]"`, `"$a[ 0]"`, `"$a['k']"`) are refused, matching PHP's
//!   parse errors rather than silently inventing a key.

use super::decode::{is_ident_start, LiteralCursor};
use super::{Token, TokenKind};
use crate::errors::EvalParseError;
use elephc_chopper::scanner::{
    braced_expression_len, braced_interpolation, BracedInterpolation, InterpolationComment,
    InterpolationError,
};
use elephc_builtin_contract::string_literal::{push_escaped_byte, push_literal_char};

impl LiteralCursor<'_> {
    /// Reads a double-quoted string literal starting at the opening quote.
    ///
    /// Returns exactly one `TokenKind::String` when the literal contains no
    /// interpolation, otherwise `( <part> . <part> ... )` as a token stream.
    pub(super) fn lex_double_quoted(&mut self, line: i64) -> Result<Vec<Token>, EvalParseError> {
        self.bump_char();
        let mut tokens: Vec<Token> = Vec::new();
        let mut current = String::new();
        let mut has_interpolation = false;
        let mut terminated = false;

        while let Some(ch) = self.peek_char() {
            if ch == '"' {
                self.bump_char();
                terminated = true;
                break;
            }
            match ch {
                '\\' => {
                    self.bump_char();
                    let Some(escaped) = self.peek_char() else {
                        return Err(EvalParseError::UnterminatedString);
                    };
                    self.bump_char();
                    self.push_double_quoted_escape(escaped, &mut current)?;
                }
                // Complex interpolation: `{` is only special when a `$` follows it.
                '{' if braced_interpolation(self.remaining().as_bytes()) == Some(BracedInterpolation::CurlyDollar) => {
                    self.bump_char();
                    let inner = self.capture_braced_expr()?;
                    let part = tokenize_interpolated_fragment(&inner, line)?;
                    has_interpolation = true;
                    push_interp_part(&mut tokens, &mut current, part, line);
                }
                '$' => {
                    // Legacy `${expr}` form: PHP 8.2 deprecates it but still evaluates it.
                    if braced_interpolation(self.remaining().as_bytes()) == Some(BracedInterpolation::DollarCurly) {
                        self.bump_char();
                        self.bump_char();
                        let inner_raw = self.capture_braced_expr()?;
                        // Re-prepend the `$` so the captured text is a valid expression.
                        let inner = format!("${inner_raw}");
                        let part = tokenize_interpolated_fragment(&inner, line)?;
                        has_interpolation = true;
                        push_interp_part(&mut tokens, &mut current, part, line);
                        continue;
                    }
                    self.bump_char();
                    // A PHP variable name may not start with a digit, so `"$2-$1"` is
                    // literal text — measured against PHP 8.5.6, which prints `$2-$1`.
                    // This matters well beyond cosmetics: `preg_replace()` back-references
                    // are written exactly that way inside double-quoted replacements.
                    let name = if self.peek_char().is_some_and(is_ident_start) {
                        self.lex_ident()
                    } else {
                        String::new()
                    };
                    if name.is_empty() {
                        current.push('$');
                        continue;
                    }
                    has_interpolation = true;
                    let mut part = vec![Token::new(TokenKind::DollarIdent(name), line)];
                    self.append_simple_access(&mut part, line)?;
                    push_interp_part(&mut tokens, &mut current, part, line);
                }
                _ => {
                    push_literal_char(ch, &mut current);
                    self.bump_char();
                }
            }
        }

        if !terminated {
            return Err(EvalParseError::UnterminatedString);
        }

        if !has_interpolation {
            return Ok(vec![Token::new(TokenKind::String(current), line)]);
        }

        if !current.is_empty() {
            tokens.push(Token::new(TokenKind::Dot, line));
            tokens.push(Token::new(TokenKind::String(current), line));
        }

        let mut result = vec![Token::new(TokenKind::LParen, line)];
        result.extend(tokens);
        result.push(Token::new(TokenKind::RParen, line));
        Ok(result)
    }

    /// Appends the single `[offset]` or `->prop` access PHP's simple interpolation
    /// syntax allows after a `$name`, leaving the cursor just past it.
    ///
    /// A `-` that is not followed by `>` and an ident-start character is left alone so it
    /// lands in the literal text, matching PHP: `"$o->1"` interpolates `$o` and keeps
    /// `->1` as text.
    fn append_simple_access(
        &mut self,
        part: &mut Vec<Token>,
        line: i64,
    ) -> Result<(), EvalParseError> {
        if self.peek_char() == Some('[') {
            self.bump_char();
            self.append_simple_offset_key(part, line)?;
            if self.peek_char() != Some(']') {
                return Err(EvalParseError::UnterminatedString);
            }
            self.bump_char();
        } else if self.peek_char() == Some('-')
            && self.peek_next_char() == Some('>')
            && self.peek_nth_char(2).is_some_and(is_ident_start)
        {
            self.bump_char();
            self.bump_char();
            let property = self.lex_ident();
            part.push(Token::new(TokenKind::Arrow, line));
            part.push(Token::new(TokenKind::Ident(property), line));
        }
        Ok(())
    }

    /// Appends the `[ key ]` tokens for a simple `"$name[offset]"` interpolation.
    ///
    /// PHP simple-syntax keys are a `$var`, an optionally negative integer, or a bareword
    /// treated as a string key. Quoted keys, whitespace and empty keys are PHP parse
    /// errors and are refused here rather than coerced into an empty-string key.
    fn append_simple_offset_key(
        &mut self,
        part: &mut Vec<Token>,
        line: i64,
    ) -> Result<(), EvalParseError> {
        part.push(Token::new(TokenKind::LBracket, line));
        match self.peek_char() {
            Some('$') => {
                self.bump_char();
                if !self.peek_char().is_some_and(is_ident_start) {
                    return Err(EvalParseError::ExpectedVariable);
                }
                let name = self.lex_ident();
                part.push(Token::new(TokenKind::DollarIdent(name), line));
            }
            Some(ch) if ch == '-' || ch.is_ascii_digit() => {
                let mut digits = String::new();
                if ch == '-' {
                    digits.push('-');
                    self.bump_char();
                }
                while let Some(digit) = self.peek_char() {
                    if !digit.is_ascii_digit() {
                        break;
                    }
                    digits.push(digit);
                    self.bump_char();
                }
                let value = digits
                    .parse::<i64>()
                    .map_err(|_| EvalParseError::InvalidNumber)?;
                part.push(Token::new(TokenKind::Int(value), line));
            }
            _ => {
                let key = self.lex_ident();
                if key.is_empty() {
                    return Err(EvalParseError::UnexpectedToken);
                }
                let mut literal = String::new();
                for ch in key.chars() { push_literal_char(ch, &mut literal); }
                part.push(Token::new(TokenKind::String(literal), line));
            }
        }
        part.push(Token::new(TokenKind::RBracket, line));
        Ok(())
    }

    /// Captures an eval expression and rejects PHP closing tags inside its comments.
    fn capture_braced_expr(&mut self) -> Result<String, EvalParseError> {
        let source = self.remaining();
        let len = braced_expression_len(source.as_bytes(), InterpolationComment::RejectCloseTag)
            .map_err(|error| match error {
                InterpolationError::CloseTag => EvalParseError::UnexpectedToken,
                _ => EvalParseError::UnterminatedString,
            })?;
        let inner = source[..len - 1].to_owned();
        self.skip_bytes(len);
        Ok(inner)
    }

    /// Appends one PHP double-quoted escape, consuming any hexadecimal, octal or `\u{…}` tail.
    fn push_double_quoted_escape(
        &mut self,
        escaped: char,
        out: &mut String,
    ) -> Result<(), EvalParseError> {
        match escaped {
            'u' => return self.push_unicode_escape(out),
            'n' => out.push('\n'),
            'r' => out.push('\r'),
            't' => out.push('\t'),
            'v' => out.push('\x0b'),
            'e' => out.push('\x1b'),
            'f' => out.push('\x0c'),
            '\\' => out.push('\\'),
            '"' => out.push('"'),
            '$' => out.push('$'),
            'x' | 'X' => {
                let mut digits = String::new();
                while digits.len() < 2
                    && self.peek_char().is_some_and(|ch| ch.is_ascii_hexdigit())
                {
                    digits.push(self.peek_char().expect("hex digit was checked"));
                    self.bump_char();
                }
                if digits.is_empty() {
                    out.push('\\');
                    out.push(escaped);
                } else {
                    let byte = u8::from_str_radix(&digits, 16)
                        .expect("one or two checked hexadecimal digits must parse");
                    push_escaped_byte(byte, out);
                }
            }
            first @ '0'..='7' => {
                let mut digits = String::from(first);
                while digits.len() < 3
                    && self.peek_char().is_some_and(|ch| matches!(ch, '0'..='7'))
                {
                    digits.push(self.peek_char().expect("octal digit was checked"));
                    self.bump_char();
                }
                let byte = u16::from_str_radix(&digits, 8)
                    .expect("checked octal digits must parse") as u8;
                push_escaped_byte(byte, out);
            }
            other => {
                out.push('\\');
                push_literal_char(other, out);
            }
        }
        Ok(())
    }

    /// Appends a `\u{…}` escape as the UTF-8 bytes of its codepoint, as the compiler's
    /// `scan_unicode_escape` does.
    ///
    /// The eval lexer had no `\u` arm, so `"\u{e9}"` stayed the six characters `\u{e9}`
    /// where reference PHP 8.5.10 gives the two bytes `c3 a9` (MEASURED). As in PHP: `\u`
    /// without a `{` is literal text; an empty, non-hex or above-U+10FFFF body is a parse
    /// error ("Invalid UTF-8 codepoint escape sequence"); a surrogate is encoded on three
    /// bytes, which a Rust `char` cannot hold.
    fn push_unicode_escape(&mut self, out: &mut String) -> Result<(), EvalParseError> {
        if self.peek_char() != Some('{') {
            out.push_str("\\u");
            return Ok(());
        }
        self.bump_char();
        let mut value = 0u32;
        let mut digits = 0;
        loop {
            let Some(ch) = self.peek_char() else {
                return Err(EvalParseError::InvalidUtf8);
            };
            self.bump_char();
            if ch == '}' {
                if digits == 0 {
                    return Err(EvalParseError::InvalidUtf8);
                }
                if let Some(scalar) = char::from_u32(value) {
                    push_literal_char(scalar, out);
                } else if (0xd800..=0xdfff).contains(&value) {
                    for byte in [
                        0xe0 | ((value >> 12) as u8),
                        0x80 | (((value >> 6) & 0x3f) as u8),
                        0x80 | ((value & 0x3f) as u8),
                    ] {
                        push_escaped_byte(byte, out);
                    }
                } else {
                    return Err(EvalParseError::InvalidUtf8);
                }
                return Ok(());
            }
            let Some(digit) = ch.to_digit(16) else {
                return Err(EvalParseError::InvalidUtf8);
            };
            value = value.saturating_mul(16).saturating_add(digit);
            digits += 1;
            if value > 0x10ffff {
                return Err(EvalParseError::InvalidUtf8);
            }
        }
    }
}

/// Appends one already-tokenized interpolation part to the running stream, flushing the
/// pending literal text and inserting the `.` concatenation operators.
///
/// The first part always emits the pending literal even when empty, so the resulting `.`
/// chain is string-typed exactly like PHP's rule that a double-quoted literal is a string.
fn push_interp_part(
    tokens: &mut Vec<Token>,
    current: &mut String,
    part: Vec<Token>,
    line: i64,
) {
    if tokens.is_empty() {
        tokens.push(Token::new(
            TokenKind::String(std::mem::take(current)),
            line,
        ));
    } else if !current.is_empty() {
        tokens.push(Token::new(TokenKind::Dot, line));
        tokens.push(Token::new(
            TokenKind::String(std::mem::take(current)),
            line,
        ));
    }
    tokens.push(Token::new(TokenKind::Dot, line));
    tokens.extend(part);
}

/// Tokenizes captured `{$expr}` source as a standalone parenthesized expression.
///
/// Recursion terminates because the captured text is strictly shorter than the enclosing
/// literal. Inner lexer errors propagate unchanged so garbage inside braces stays a parse
/// error instead of becoming silently accepted text.
fn tokenize_interpolated_fragment(
    inner: &str,
    line: i64,
) -> Result<Vec<Token>, EvalParseError> {
    let fragment = super::tokenize(inner)?;
    let mut part = vec![Token::new(TokenKind::LParen, line)];
    part.extend(
        fragment
            .into_iter()
            .filter(|token| !matches!(token.kind(), TokenKind::Eof | TokenKind::DocComment(_)))
            .map(|token| Token::new(token.into_kind(), line)),
    );
    part.push(Token::new(TokenKind::RParen, line));
    Ok(part)
}
