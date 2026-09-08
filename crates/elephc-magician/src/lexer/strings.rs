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
//! - Simple, hexadecimal and octal escapes retain PHP bytes, including non-UTF-8 values.
//! - Escaped dollars remain literal; compile warnings keep their source ordering.
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
        let mut warnings = Vec::new();
        let mut current = Vec::new();
        let mut has_interpolation = false;
        let mut terminated = false;

        let scanned = (|| {
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
                        if ('0'..='7').contains(&escaped) {
                            let mut byte = escaped as u16 - '0' as u16;
                            let mut digits = escaped.to_string();
                            for _ in 0..2 {
                                let Some(next @ '0'..='7') = self.peek_char() else { break; };
                                byte = byte * 8 + next as u16 - '0' as u16;
                                digits.push(next);
                                self.bump_char();
                            }
                            if byte > 255 {
                                warnings.push(Token::new(TokenKind::CompileWarning(crate::eval_ir::EvalCompileWarning {
                                    message: format!("Octal escape sequence overflow \\{digits} is greater than \\377"),
                                    line: self.line,
                                }), self.line));
                            }
                            current.push(byte as u8);
                        } else {
                            self.push_double_quoted_escape(escaped, &mut current);
                        }
                    }
                    // Complex interpolation: `{` is only special when a `$` follows it.
                    '{' if self.peek_next_char() == Some('$') => {
                        self.bump_char();
                        let warning_line = self.line;
                        let inner = self.capture_braced_expr()?;
                        let part = tokenize_interpolated_fragment(&inner, line, warning_line)?;
                        has_interpolation = true;
                        push_interp_part(&mut tokens, &mut current, part, line, &mut warnings);
                    }
                    '$' => {
                        // Legacy `${expr}` form: PHP 8.2 deprecates it but still evaluates it.
                        if self.peek_next_char() == Some('{') {
                            self.bump_char();
                            self.bump_char();
                            let warning_line = self.line;
                            let inner_raw = self.capture_braced_expr()?;
                            // Re-prepend the `$` so the captured text is a valid expression.
                            let inner = format!("${inner_raw}");
                            let part = tokenize_interpolated_fragment(&inner, line, warning_line)?;
                            has_interpolation = true;
                            push_interp_part(&mut tokens, &mut current, part, line, &mut warnings);
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
                            current.push(b'$');
                            continue;
                        }
                        has_interpolation = true;
                        let mut part = vec![Token::new(TokenKind::DollarIdent(name), line)];
                        self.append_simple_access(&mut part, line)?;
                        push_interp_part(&mut tokens, &mut current, part, line, &mut warnings);
                    }
                    _ => {
                        let mut bytes = [0; 4];
                        current.extend_from_slice(ch.encode_utf8(&mut bytes).as_bytes());
                        self.bump_char();
                    }
                }
            }

            if !terminated {
                return Err(EvalParseError::UnterminatedString);
            }
            Ok(())
        })();

        scanned.map_err(|error: EvalParseError| {
            error.with_compile_warnings(warnings.iter().filter_map(|token| match token.kind() {
                TokenKind::CompileWarning(warning) => Some(warning.clone()),
                _ => None,
            }).collect())
        })?;

        if !has_interpolation {
            warnings.push(Token::new(literal_token(current), line));
            return Ok(warnings);
        }

        if !current.is_empty() {
            tokens.push(Token::new(TokenKind::Dot, line));
            tokens.push(Token::new(literal_token(current), line));
        }

        let mut result = vec![Token::new(TokenKind::LParen, line)];
        result.extend(tokens);
        result.push(Token::new(TokenKind::RParen, line));
        warnings.extend(result);
        Ok(warnings)
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

    /// Appends simple and hexadecimal PHP escapes as bytes; octal warnings are handled by the scanner.
    fn push_double_quoted_escape(&mut self, escaped: char, out: &mut Vec<u8>) {
        match escaped {
            'n' => out.push(b'\n'),
            'r' => out.push(b'\r'),
            't' => out.push(b'\t'),
            'v' => out.push(0x0b),
            'e' => out.push(0x1b),
            'f' => out.push(0x0c),
            '\\' => out.push(b'\\'),
            '"' => out.push(b'"'),
            '$' => out.push(b'$'),
            'x' | 'X' => {
                let mut byte = 0_u8;
                let mut count = 0;
                while count < 2 {
                    let Some(digit) = self.peek_char().and_then(|ch| ch.to_digit(16)) else { break; };
                    byte = byte * 16 + digit as u8;
                    count += 1;
                    self.bump_char();
                }
                if count == 0 {
                    out.extend_from_slice(&[b'\\', escaped as u8]);
                } else {
                    out.push(byte);
                }
            }
            other => {
                out.push(b'\\');
                let mut bytes = [0; 4];
                out.extend_from_slice(other.encode_utf8(&mut bytes).as_bytes());
            }
        }
    }
}

/// Preserves binary literals while keeping the existing UTF-8 token shape where possible.
fn literal_token(bytes: Vec<u8>) -> TokenKind {
    match String::from_utf8(bytes) {
        Ok(text) => TokenKind::String(text),
        Err(error) => TokenKind::ByteString(error.into_bytes()),
    }
}

/// Appends one already-tokenized interpolation part to the running stream, flushing the
/// pending literal text and inserting the `.` concatenation operators.
///
/// The first part always emits the pending literal even when empty, so the resulting `.`
/// chain is string-typed exactly like PHP's rule that a double-quoted literal is a string.
fn push_interp_part(
    tokens: &mut Vec<Token>,
    current: &mut Vec<u8>,
    mut part: Vec<Token>,
    line: i64,
    warnings: &mut Vec<Token>,
) {
    part.retain(|token| {
        if matches!(token.kind(), TokenKind::CompileWarning(_)) {
            warnings.push(token.clone());
            false
        } else { true }
    });
    if tokens.is_empty() {
        tokens.push(Token::new(
            literal_token(std::mem::take(current)),
            line,
        ));
    } else if !current.is_empty() {
        tokens.push(Token::new(TokenKind::Dot, line));
        tokens.push(Token::new(
            literal_token(std::mem::take(current)),
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
    warning_line: i64,
) -> Result<Vec<Token>, EvalParseError> {
    let fragment = tokenize(inner).map_err(|mut error| {
        if let EvalParseError::WithCompileWarnings { warnings, .. } = &mut error {
            for warning in warnings {
                warning.line += warning_line - 1;
            }
        }
        error
    })?;
    let mut part = vec![Token::new(TokenKind::LParen, line)];
    part.extend(
        fragment
            .into_iter()
            .filter(|token| *token.kind() != TokenKind::Eof)
            .map(|token| {
                let mut kind = token.into_kind();
                if let TokenKind::CompileWarning(warning) = &mut kind {
                    warning.line += warning_line - 1;
                }
                Token::new(kind, line)
            }),
    );
    part.push(Token::new(TokenKind::RParen, line));
    Ok(part)
}
