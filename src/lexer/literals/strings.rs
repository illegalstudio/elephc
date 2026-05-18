//! Purpose:
//! Scans PHP string literal forms, including interpolation-aware double strings and heredocs.
//! Emits string tokens plus interpolation boundary tokens that the parser can consume.
//!
//! Called from:
//! - `crate::lexer::scan` through `crate::lexer::literals`.
//!
//! Key details:
//! - Escape and interpolation behavior must preserve PHP-compatible text and source spans.

use super::super::cursor::Cursor;
use super::super::token::Token;
use crate::errors::CompileError;
use crate::span::Span;
use std::iter::Peekable;
use std::str::Chars;

trait EscapeInput {
    fn peek_escape(&mut self) -> Option<char>;
    fn advance_escape(&mut self) -> Option<char>;
}

impl EscapeInput for Cursor<'_> {
    fn peek_escape(&mut self) -> Option<char> {
        self.peek()
    }

    fn advance_escape(&mut self) -> Option<char> {
        self.advance()
    }
}

struct CharsEscapeInput<'a, 'b> {
    chars: &'a mut Peekable<Chars<'b>>,
}

impl EscapeInput for CharsEscapeInput<'_, '_> {
    fn peek_escape(&mut self) -> Option<char> {
        self.chars.peek().copied()
    }

    fn advance_escape(&mut self) -> Option<char> {
        self.chars.next()
    }
}

/// Scan a double-quoted string with interpolation support.
/// Returns one or more tokens: for `"Hello $name!"` it returns
/// `StringLiteral("Hello ") . Variable("name") . StringLiteral("!")`
/// (with Dot tokens for concatenation).
pub(in crate::lexer) fn scan_double_string_interpolated(
    cursor: &mut Cursor,
) -> Result<Vec<(Token, Span)>, CompileError> {
    let span = cursor.span();
    cursor.advance(); // opening "

    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut has_interpolation = false;

    loop {
        match cursor.peek() {
            Some('"') => {
                cursor.advance();
                break;
            }
            Some('\\') => {
                cursor.advance();
                let escaped = scan_double_quoted_escape(cursor, span, MissingEscape::Error)?;
                current.push_str(&escaped);
            }
            Some('$') => {
                cursor.advance(); // consume '$'
                let mut name = String::new();
                while let Some(ch) = cursor.peek() {
                    if ch.is_ascii_alphanumeric() || ch == '_' {
                        name.push(ch);
                        cursor.advance();
                    } else {
                        break;
                    }
                }
                if name.is_empty() {
                    current.push('$');
                } else {
                    has_interpolation = true;
                    if !current.is_empty() || tokens.is_empty() {
                        if !tokens.is_empty() {
                            tokens.push((Token::Dot, span));
                        }
                        tokens.push((Token::StringLiteral(std::mem::take(&mut current)), span));
                    }
                    if !tokens.is_empty() && !matches!(tokens.last(), Some((Token::Dot, _))) {
                        tokens.push((Token::Dot, span));
                    }
                    tokens.push((Token::Variable(name), span));
                }
            }
            Some(c) => {
                push_literal_char(c, &mut current);
                cursor.advance();
            }
            None => return Err(CompileError::new(span, "Unterminated string literal")),
        }
    }

    if !has_interpolation {
        return Ok(vec![(Token::StringLiteral(current), span)]);
    }

    if !current.is_empty() {
        tokens.push((Token::Dot, span));
        tokens.push((Token::StringLiteral(current), span));
    }

    let mut result = vec![(Token::LParen, span)];
    result.extend(tokens);
    result.push((Token::RParen, span));
    Ok(result)
}

pub(in crate::lexer) fn scan_single_string(cursor: &mut Cursor) -> Result<Token, CompileError> {
    let span = cursor.span();
    cursor.advance(); // opening '

    let mut value = String::new();

    loop {
        match cursor.advance() {
            Some('\'') => return Ok(Token::StringLiteral(value)),
            Some('\\') => match cursor.peek() {
                Some('\'') => {
                    cursor.advance();
                    value.push('\'');
                }
                Some('\\') => {
                    cursor.advance();
                    value.push('\\');
                }
                _ => value.push('\\'),
            },
            Some(c) => push_literal_char(c, &mut value),
            None => return Err(CompileError::new(span, "Unterminated string literal")),
        }
    }
}

/// Scan a heredoc or nowdoc string.
/// At this point, `<<<` has already been consumed.
/// Heredoc: `<<<LABEL` or `<<<"LABEL"` — supports variable interpolation like double-quoted strings
/// Nowdoc: `<<<'LABEL'` — no interpolation (like single-quoted strings)
pub(in crate::lexer) fn scan_heredoc(
    cursor: &mut Cursor,
) -> Result<Vec<(Token, Span)>, CompileError> {
    let span = cursor.span();

    while cursor.peek() == Some(' ') || cursor.peek() == Some('\t') {
        cursor.advance();
    }

    let is_nowdoc = cursor.peek() == Some('\'');
    let is_quoted_heredoc = cursor.peek() == Some('"');

    if is_nowdoc || is_quoted_heredoc {
        cursor.advance();
    }

    let mut label = String::new();
    while let Some(ch) = cursor.peek() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            label.push(ch);
            cursor.advance();
        } else {
            break;
        }
    }

    if label.is_empty() {
        return Err(CompileError::new(span, "Expected heredoc/nowdoc label after '<<<'"));
    }

    if is_nowdoc {
        if cursor.peek() != Some('\'') {
            return Err(CompileError::new(span, "Expected closing ' for nowdoc label"));
        }
        cursor.advance();
    } else if is_quoted_heredoc {
        if cursor.peek() != Some('"') {
            return Err(CompileError::new(span, "Expected closing \" for heredoc label"));
        }
        cursor.advance();
    }

    if cursor.peek() == Some('\r') {
        cursor.advance();
    }
    if cursor.peek() == Some('\n') {
        cursor.advance();
    } else {
        return Err(CompileError::new(span, "Expected newline after heredoc/nowdoc label"));
    }

    let mut content = String::new();
    loop {
        if cursor.is_eof() {
            return Err(CompileError::new(span, "Unterminated heredoc/nowdoc"));
        }

        let remaining = cursor.remaining();

        let mut ws_count = 0;
        for b in remaining.bytes() {
            if b == b' ' || b == b'\t' {
                ws_count += 1;
            } else {
                break;
            }
        }

        let after_ws = &remaining[ws_count..];
        if after_ws.starts_with(&label) {
            let after_label = &after_ws[label.len()..];
            if after_label.is_empty()
                || after_label.starts_with(';')
                || after_label.starts_with('\n')
                || after_label.starts_with('\r')
            {
                for _ in 0..ws_count {
                    cursor.advance();
                }
                for _ in 0..label.len() {
                    cursor.advance();
                }

                if content.ends_with('\n') {
                    content.pop();
                    if content.ends_with('\r') {
                        content.pop();
                    }
                }

                if is_nowdoc {
                    return Ok(vec![(Token::StringLiteral(content), span)]);
                }

                return interpolate_heredoc_content(&content, span);
            }
        }

        match cursor.advance() {
            Some(ch) => push_literal_char(ch, &mut content),
            None => return Err(CompileError::new(span, "Unterminated heredoc/nowdoc")),
        }
    }
}

/// Interpolate variables and process escape sequences in heredoc content.
/// Handles both in a single pass so that `\$` produces a literal `$` without triggering
/// variable interpolation. Scans for `$identifier` patterns and expands them into
/// concatenation tokens: `Hello $name!` -> `("Hello " . $name . "!")`
fn interpolate_heredoc_content(
    content: &str,
    span: Span,
) -> Result<Vec<(Token, Span)>, CompileError> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut has_interpolation = false;
    let mut chars = content.chars().peekable();

    loop {
        match chars.peek() {
            None => break,
            Some(&'\\') => {
                chars.next();
                let mut input = CharsEscapeInput { chars: &mut chars };
                let escaped = scan_double_quoted_escape(&mut input, span, MissingEscape::Literal)?;
                current.push_str(&escaped);
            }
            Some(&'$') => {
                chars.next();
                let mut name = String::new();
                while let Some(&ch) = chars.peek() {
                    if ch.is_ascii_alphanumeric() || ch == '_' {
                        name.push(ch);
                        chars.next();
                    } else {
                        break;
                    }
                }
                if name.is_empty() {
                    current.push('$');
                } else {
                    has_interpolation = true;
                    if !current.is_empty() || tokens.is_empty() {
                        if !tokens.is_empty() {
                            tokens.push((Token::Dot, span));
                        }
                        tokens.push((
                            Token::StringLiteral(std::mem::take(&mut current)),
                            span,
                        ));
                    }
                    if !tokens.is_empty() && !matches!(tokens.last(), Some((Token::Dot, _))) {
                        tokens.push((Token::Dot, span));
                    }
                    tokens.push((Token::Variable(name), span));
                }
            }
            Some(&ch) => {
                push_literal_char(ch, &mut current);
                chars.next();
            }
        }
    }

    if !has_interpolation {
        return Ok(vec![(Token::StringLiteral(current), span)]);
    }

    if !current.is_empty() {
        tokens.push((Token::Dot, span));
        tokens.push((Token::StringLiteral(current), span));
    }

    let mut result = vec![(Token::LParen, span)];
    result.extend(tokens);
    result.push((Token::RParen, span));
    Ok(result)
}

#[derive(Clone, Copy)]
enum MissingEscape {
    Error,
    Literal,
}

fn scan_double_quoted_escape(
    input: &mut impl EscapeInput,
    span: Span,
    missing_escape: MissingEscape,
) -> Result<String, CompileError> {
    let Some(ch) = input.advance_escape() else {
        return match missing_escape {
            MissingEscape::Error => Err(CompileError::new(span, "Unterminated string literal")),
            MissingEscape::Literal => Ok("\\".to_string()),
        };
    };

    let mut out = String::new();
    match ch {
        'n' => out.push('\n'),
        'r' => out.push('\r'),
        't' => out.push('\t'),
        'v' => out.push('\u{0b}'),
        'e' => out.push('\u{1b}'),
        'f' => out.push('\u{0c}'),
        '\\' => out.push('\\'),
        '"' => out.push('"'),
        '$' => out.push('$'),
        'x' => scan_hex_escape(input, &mut out),
        'u' => scan_unicode_escape(input, span, &mut out)?,
        '0'..='7' => scan_octal_escape(input, ch, &mut out),
        c => {
            out.push('\\');
            push_literal_char(c, &mut out);
        }
    }
    Ok(out)
}

fn scan_hex_escape(input: &mut impl EscapeInput, out: &mut String) {
    let mut value = 0u32;
    let mut digits = 0;
    while digits < 2 {
        let Some(ch) = input.peek_escape() else {
            break;
        };
        let Some(digit) = ch.to_digit(16) else {
            break;
        };
        input.advance_escape();
        value = (value << 4) | digit;
        digits += 1;
    }

    if digits == 0 {
        out.push('\\');
        out.push('x');
    } else {
        push_byte_escape(value as u8, out);
    }
}

fn scan_octal_escape(input: &mut impl EscapeInput, first: char, out: &mut String) {
    let mut value = first.to_digit(8).unwrap_or(0);
    let mut digits = 1;
    while digits < 3 {
        let Some(ch) = input.peek_escape() else {
            break;
        };
        let Some(digit) = ch.to_digit(8) else {
            break;
        };
        input.advance_escape();
        value = (value << 3) | digit;
        digits += 1;
    }
    push_byte_escape((value & 0xff) as u8, out);
}

fn scan_unicode_escape(
    input: &mut impl EscapeInput,
    span: Span,
    out: &mut String,
) -> Result<(), CompileError> {
    if input.peek_escape() != Some('{') {
        out.push('\\');
        out.push('u');
        return Ok(());
    }
    input.advance_escape();

    let mut value = 0u32;
    let mut digits = 0;
    loop {
        let Some(ch) = input.advance_escape() else {
            return Err(CompileError::new(
                span,
                "Invalid UTF-8 codepoint escape sequence",
            ));
        };
        if ch == '}' {
            if digits == 0 {
                return Err(CompileError::new(
                    span,
                    "Invalid UTF-8 codepoint escape sequence",
                ));
            }
            if let Some(scalar) = char::from_u32(value) {
                push_literal_char(scalar, out);
            } else if (0xd800..=0xdfff).contains(&value) {
                push_codepoint_utf8_bytes(value, out);
            } else {
                return Err(CompileError::new(
                    span,
                    "Invalid UTF-8 codepoint escape sequence",
                ));
            }
            return Ok(());
        }
        let Some(digit) = ch.to_digit(16) else {
            return Err(CompileError::new(
                span,
                "Invalid UTF-8 codepoint escape sequence",
            ));
        };
        value = value.saturating_mul(16).saturating_add(digit);
        digits += 1;
        if value > 0x10ffff {
            return Err(CompileError::new(
                span,
                "Invalid UTF-8 codepoint escape sequence",
            ));
        }
    }
}

fn push_codepoint_utf8_bytes(codepoint: u32, out: &mut String) {
    let bytes = [
        0xe0 | ((codepoint >> 12) as u8),
        0x80 | (((codepoint >> 6) & 0x3f) as u8),
        0x80 | ((codepoint & 0x3f) as u8),
    ];
    for byte in bytes {
        push_byte_escape(byte, out);
    }
}

fn push_byte_escape(byte: u8, out: &mut String) {
    crate::string_bytes::push_escaped_byte(byte, out);
}

fn push_literal_char(ch: char, out: &mut String) {
    crate::string_bytes::push_literal_char(ch, out);
}
