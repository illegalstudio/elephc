//! Purpose:
//! Expands double-quoted eval-fragment string literals into interpolation-aware token
//! streams. A literal without interpolation stays exactly one `TokenKind::String`; a
//! literal with interpolation becomes a parenthesized `.` concatenation the existing
//! grammar already lowers to `EvalBinOp::Concat`.
//!
//! Called from:
//! - `super::scan::Lexer::next_tokens()` for every `"` encountered in a fragment.
//!
//! Key details:
//! - Escape handling covers PHP's simple, hexadecimal, and octal forms; `\$` still
//!   yields a literal `$` and never interpolates.
//! - Every synthetic token carries the line of the opening quote, keeping `__LINE__`
//!   stable across multi-line literals.
//! - PHP simple syntax allows exactly one `[offset]` or `->prop` after `$name`; anything
//!   deeper requires the complex `{$expr}` form.
//! - Malformed offsets (`"$a[]"`, `"$a[ 0]"`, `"$a['k']"`) are refused, matching PHP's
//!   parse errors rather than silently inventing a key.

use super::scan::{is_ident_start, tokenize, Lexer};
use super::{Token, TokenKind};
use crate::errors::EvalParseError;

impl Lexer<'_> {
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
                    self.push_double_quoted_escape(escaped, &mut current);
                }
                // Complex interpolation: `{` is only special when a `$` follows it.
                '{' if self.peek_next_char() == Some('$') => {
                    self.bump_char();
                    let inner = self.capture_braced_expr()?;
                    let part = tokenize_interpolated_fragment(&inner, line)?;
                    has_interpolation = true;
                    push_interp_part(&mut tokens, &mut current, part, line);
                }
                '$' => {
                    // Legacy `${expr}` form: PHP 8.2 deprecates it but still evaluates it.
                    if self.peek_next_char() == Some('{') {
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
                    current.push(ch);
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
                part.push(Token::new(TokenKind::String(key), line));
            }
        }
        part.push(Token::new(TokenKind::RBracket, line));
        Ok(())
    }

    /// Captures the raw source text of a `{$expr}` interpolation up to its matching `}`.
    ///
    /// The opening `{` is already consumed and the closing `}` is consumed here. Nested
    /// braces are balanced and quoted sections are copied verbatim so braces inside a
    /// nested string literal never change the depth.
    fn capture_braced_expr(&mut self) -> Result<String, EvalParseError> {
        let mut inner = String::new();
        let mut depth = 1usize;
        loop {
            let Some(ch) = self.peek_char() else {
                return Err(EvalParseError::UnterminatedString);
            };
            self.bump_char();
            match ch {
                '{' => {
                    depth += 1;
                    inner.push('{');
                }
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        return Ok(inner);
                    }
                    inner.push('}');
                }
                quote @ ('"' | '\'') => {
                    inner.push(quote);
                    self.capture_braced_string(quote, &mut inner)?;
                }
                other => inner.push(other),
            }
        }
    }

    /// Copies a nested string literal inside a `{$expr}` capture verbatim, including its
    /// escape sequences and its closing quote.
    fn capture_braced_string(
        &mut self,
        quote: char,
        inner: &mut String,
    ) -> Result<(), EvalParseError> {
        loop {
            let Some(ch) = self.peek_char() else {
                return Err(EvalParseError::UnterminatedString);
            };
            self.bump_char();
            if ch == '\\' {
                inner.push('\\');
                let Some(escaped) = self.peek_char() else {
                    return Err(EvalParseError::UnterminatedString);
                };
                inner.push(escaped);
                self.bump_char();
                continue;
            }
            inner.push(ch);
            if ch == quote {
                return Ok(());
            }
        }
    }

    /// Appends one PHP double-quoted escape, consuming any hexadecimal or octal tail.
    fn push_double_quoted_escape(&mut self, escaped: char, out: &mut String) {
        match escaped {
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
                    out.push(char::from(byte));
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
                out.push(char::from(byte));
            }
            other => {
                out.push('\\');
                out.push(other);
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
    let fragment = tokenize(inner)?;
    let mut part = vec![Token::new(TokenKind::LParen, line)];
    part.extend(
        fragment
            .into_iter()
            .filter(|token| *token.kind() != TokenKind::Eof)
            .map(|token| Token::new(token.into_kind(), line)),
    );
    part.push(Token::new(TokenKind::RParen, line));
    Ok(part)
}
