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
use super::identifiers::is_ident_continue;
use crate::errors::CompileError;
use crate::span::Span;
use std::iter::Peekable;
use std::str::Chars;

/// Abstracts character lookahead and consumption for escape sequence and interpolation
/// parsers, so the double-quoted (cursor-backed) and heredoc (chars-backed) paths share
/// one implementation.
trait EscapeInput {
    /// Returns the next character without consuming it.
    fn peek_escape(&mut self) -> Option<char>;
    /// Consumes and returns the next character.
    fn advance_escape(&mut self) -> Option<char>;
    /// Returns the character `n` positions ahead (0 = next) without consuming, for the
    /// multi-character lookahead interpolation needs (`{$`, `->prop`).
    fn peek_nth(&mut self, n: usize) -> Option<char>;
}

impl EscapeInput for Cursor<'_> {
    /// Returns the next character from the cursor without advancing.
    fn peek_escape(&mut self) -> Option<char> {
        self.peek()
    }
    /// Consumes and returns the next character from the cursor.
    fn advance_escape(&mut self) -> Option<char> {
        self.advance()
    }
    /// Returns the character `n` positions ahead in the remaining source.
    fn peek_nth(&mut self, n: usize) -> Option<char> {
        self.remaining().chars().nth(n)
    }
}

struct CharsEscapeInput<'a, 'b> {
    /// Borrows a peekableChars iterator to provide EscapeInput behavior.
    chars: &'a mut Peekable<Chars<'b>>,
}

impl EscapeInput for CharsEscapeInput<'_, '_> {
    /// Returns the next character from the peekable iterator without advancing.
    fn peek_escape(&mut self) -> Option<char> {
        self.chars.peek().copied()
    }

    /// Consumes and returns the next character from the peekable iterator.
    fn advance_escape(&mut self) -> Option<char> {
        self.chars.next()
    }

    /// Returns the character `n` positions ahead by cloning the peekable iterator, which
    /// is cheap (a `Chars` slice cursor) and leaves the original position untouched.
    fn peek_nth(&mut self, n: usize) -> Option<char> {
        (*self.chars).clone().nth(n)
    }
}

/// Scan a double-quoted string with interpolation support.
/// Returns one or more tokens: for `"Hello $name!"` it returns
/// `("Hello " . Variable("name") . "!")` (parenthesized, with Dot tokens for concatenation).
pub(in crate::lexer) fn scan_double_string_interpolated(
    cursor: &mut Cursor,
) -> Result<Vec<(Token, Span)>, CompileError> {
    let span = cursor.span();
    cursor.advance(); // opening "
    interpolate(cursor, span, Some('"'), MissingEscape::Error)
}

/// Appends one interpolated expression part (already a token list) to the running stream,
/// flushing any pending literal text and inserting `.` concatenation.
///
/// When this is the first part, the pending literal (possibly empty) is emitted first so
/// the resulting `.` chain is always string-typed, matching PHP's rule that a
/// double-quoted/heredoc string is always a string.
fn push_interp_part(
    tokens: &mut Vec<(Token, Span)>,
    current: &mut String,
    part: Vec<Token>,
    span: Span,
) {
    if tokens.is_empty() {
        tokens.push((Token::StringLiteral(std::mem::take(current)), span));
    } else if !current.is_empty() {
        tokens.push((Token::Dot, span));
        tokens.push((Token::StringLiteral(std::mem::take(current)), span));
    }
    tokens.push((Token::Dot, span));
    for token in part {
        tokens.push((token, span));
    }
}

/// Shared interpolation routine for double-quoted strings and heredoc bodies.
///
/// `terminator` is the closing delimiter (`Some('"')` for double-quoted strings, `None`
/// for heredoc content that ends at input end). Handles escapes, simple `$name`,
/// `$name[offset]` and `$name->prop` syntax, and complex `{$expr}` interpolation. Returns
/// a single `StringLiteral` when no interpolation occurred, otherwise a parenthesized
/// concatenation token stream.
fn interpolate(
    input: &mut impl EscapeInput,
    span: Span,
    terminator: Option<char>,
    missing_escape: MissingEscape,
) -> Result<Vec<(Token, Span)>, CompileError> {
    let mut tokens: Vec<(Token, Span)> = Vec::new();
    let mut current = String::new();
    let mut has_interpolation = false;

    loop {
        match input.peek_escape() {
            None => {
                if terminator.is_some() {
                    return Err(CompileError::new(span, "Unterminated string literal"));
                }
                break;
            }
            Some(c) if Some(c) == terminator => {
                input.advance_escape();
                break;
            }
            Some('\\') => {
                input.advance_escape();
                let escaped = scan_double_quoted_escape(input, span, missing_escape)?;
                current.push_str(&escaped);
            }
            // Complex interpolation: `{$expr}` (the `{` is only special when followed by `$`).
            Some('{') if input.peek_nth(1) == Some('$') => {
                input.advance_escape(); // consume '{'
                let inner = capture_braced_expr(input, span)?;
                let fragment = tokenize_fragment(&inner, span)?;
                has_interpolation = true;
                let mut part = vec![Token::LParen];
                part.extend(fragment.into_iter().map(|(token, _)| token));
                part.push(Token::RParen);
                push_interp_part(&mut tokens, &mut current, part, span);
            }
            Some('$') => {
                input.advance_escape(); // consume '$'
                let mut name = String::new();
                while let Some(ch) = input.peek_escape() {
                    if is_ident_continue(ch) {
                        name.push(ch);
                        input.advance_escape();
                    } else {
                        break;
                    }
                }
                if name.is_empty() {
                    current.push('$');
                } else {
                    has_interpolation = true;
                    let mut part = vec![Token::Variable(name)];
                    append_simple_access(input, &mut part, span)?;
                    push_interp_part(&mut tokens, &mut current, part, span);
                }
            }
            Some(c) => {
                push_literal_char(c, &mut current);
                input.advance_escape();
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

/// Appends the simple-syntax access that may follow a `$name` in an interpolated string:
/// a single `[offset]` or a single `->prop`. PHP's "simple syntax" allows exactly one
/// level; anything more must use the complex `{$expr}` form. Leaves `input` positioned
/// after the consumed access (or unchanged if none applies).
fn append_simple_access(
    input: &mut impl EscapeInput,
    part: &mut Vec<Token>,
    span: Span,
) -> Result<(), CompileError> {
    if input.peek_escape() == Some('[') {
        input.advance_escape(); // consume '['
        append_simple_offset_key(input, part);
        if input.peek_escape() == Some(']') {
            input.advance_escape();
        } else {
            return Err(CompileError::new(
                span,
                "Unterminated array offset in string interpolation",
            ));
        }
    } else if input.peek_escape() == Some('-')
        && input.peek_nth(1) == Some('>')
        && input.peek_nth(2).is_some_and(is_ident_continue)
    {
        input.advance_escape(); // consume '-'
        input.advance_escape(); // consume '>'
        let mut prop = String::new();
        while let Some(ch) = input.peek_escape() {
            if is_ident_continue(ch) {
                prop.push(ch);
                input.advance_escape();
            } else {
                break;
            }
        }
        part.push(Token::Arrow);
        part.push(Token::Identifier(prop));
    }
    Ok(())
}

/// Reads the offset key for a simple `$name[offset]` interpolation and appends the
/// `[ key ]` tokens to `part`. PHP simple syntax keys are a `$var`, an optionally-negative
/// integer, or a bareword treated as a string key (never quoted).
fn append_simple_offset_key(input: &mut impl EscapeInput, part: &mut Vec<Token>) {
    part.push(Token::LBracket);
    match input.peek_escape() {
        Some('$') => {
            input.advance_escape();
            let mut name = String::new();
            while let Some(ch) = input.peek_escape() {
                if is_ident_continue(ch) {
                    name.push(ch);
                    input.advance_escape();
                } else {
                    break;
                }
            }
            part.push(Token::Variable(name));
        }
        Some(c) if c == '-' || c.is_ascii_digit() => {
            let mut digits = String::new();
            if c == '-' {
                digits.push('-');
                input.advance_escape();
            }
            while let Some(ch) = input.peek_escape() {
                if ch.is_ascii_digit() {
                    digits.push(ch);
                    input.advance_escape();
                } else {
                    break;
                }
            }
            part.push(Token::IntLiteral(digits.parse().unwrap_or(0)));
        }
        _ => {
            let mut key = String::new();
            while let Some(ch) = input.peek_escape() {
                if is_ident_continue(ch) {
                    key.push(ch);
                    input.advance_escape();
                } else {
                    break;
                }
            }
            part.push(Token::StringLiteral(key));
        }
    }
    part.push(Token::RBracket);
}

/// Captures the source text of a complex `{$expr}` interpolation up to its matching `}`.
///
/// The opening `{` has already been consumed; the leading `$` is still pending and is
/// included in the returned text. Nested braces are balanced, and string literals inside
/// the expression are copied verbatim so their braces/quotes do not affect the depth.
fn capture_braced_expr(
    input: &mut impl EscapeInput,
    span: Span,
) -> Result<String, CompileError> {
    let mut inner = String::new();
    let mut depth = 1usize;
    loop {
        match input.advance_escape() {
            None => {
                return Err(CompileError::new(
                    span,
                    "Unterminated complex interpolation '{$...}'",
                ))
            }
            Some('{') => {
                depth += 1;
                inner.push('{');
            }
            Some('}') => {
                depth -= 1;
                if depth == 0 {
                    break;
                }
                inner.push('}');
            }
            Some(quote @ ('"' | '\'')) => {
                inner.push(quote);
                loop {
                    match input.advance_escape() {
                        None => {
                            return Err(CompileError::new(
                                span,
                                "Unterminated string in complex interpolation '{$...}'",
                            ))
                        }
                        Some('\\') => {
                            inner.push('\\');
                            if let Some(escaped) = input.advance_escape() {
                                inner.push(escaped);
                            }
                        }
                        Some(c) if c == quote => {
                            inner.push(c);
                            break;
                        }
                        Some(c) => inner.push(c),
                    }
                }
            }
            Some(c) => inner.push(c),
        }
    }
    Ok(inner)
}

/// Tokenizes the captured `{$expr}` source as a standalone expression by lexing it behind
/// a synthetic `<?php` tag, then dropping the open tag and EOF and re-spanning the tokens
/// to the enclosing string's span. Reuses the full lexer so nested strings, calls, and
/// array/property access inside the braces are handled like any other expression.
fn tokenize_fragment(inner: &str, span: Span) -> Result<Vec<(Token, Span)>, CompileError> {
    let source = format!("<?php {}", inner);
    let tokens = crate::lexer::tokenize(&source)?;
    Ok(tokens
        .into_iter()
        .filter(|(token, _)| !matches!(token, Token::OpenTag | Token::Eof))
        .map(|(token, _)| (token, span))
        .collect())
}

/// Scans a single-quoted PHP string, handling `\'` and `\\` escape sequences.
/// Advances past the opening `'` and stops at the closing `'`. Returns
/// `Token::StringLiteral` with the unescaped content, or an error on EOF.
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
    let mut at_line_start = true;
    loop {
        if cursor.is_eof() {
            return Err(CompileError::new(span, "Unterminated heredoc/nowdoc"));
        }

        // The closing label is only recognized at the start of a line (optionally indented),
        // so a label appearing mid-line is treated as body content.
        if at_line_start {
            let remaining = cursor.remaining();
            let ws_count = remaining
                .bytes()
                .take_while(|&b| b == b' ' || b == b'\t')
                .count();
            let after_ws = &remaining[ws_count..];
            if after_ws.starts_with(&label) {
                let after_label = &after_ws[label.len()..];
                // PHP closes the heredoc when the label is followed by end-of-input or any
                // character that cannot continue an identifier (`;`, `)`, `,`, `.`, space,
                // `[`, newline, ...) — but not by another identifier char (e.g. `EOTX`).
                let closes = after_label
                    .chars()
                    .next()
                    .map_or(true, |c| !is_ident_continue(c));
                if closes {
                    for _ in 0..ws_count + label.len() {
                        cursor.advance();
                    }

                    if content.ends_with('\n') {
                        content.pop();
                        if content.ends_with('\r') {
                            content.pop();
                        }
                    }

                    // PHP 7.3+ flexible heredoc/nowdoc: strip the closing marker's
                    // indentation from every body line before interpolation.
                    let content = strip_heredoc_indentation(&content, ws_count, span)?;

                    if is_nowdoc {
                        return Ok(vec![(Token::StringLiteral(content), span)]);
                    }

                    let mut chars = content.chars().peekable();
                    let mut input = CharsEscapeInput { chars: &mut chars };
                    return interpolate(&mut input, span, None, MissingEscape::Literal);
                }
            }
        }

        match cursor.advance() {
            Some(ch) => {
                at_line_start = ch == '\n';
                push_literal_char(ch, &mut content);
            }
            None => return Err(CompileError::new(span, "Unterminated heredoc/nowdoc")),
        }
    }
}

/// Strips the closing marker's indentation (`indent` leading space/tab characters) from
/// every line of a PHP 7.3+ flexible heredoc/nowdoc body.
///
/// A non-blank line indented by fewer than `indent` whitespace characters is a PHP
/// "Invalid body indentation level" error. Blank lines keep whatever they had (after
/// removing up to `indent` leading whitespace). `\r` line endings are preserved.
fn strip_heredoc_indentation(
    content: &str,
    indent: usize,
    span: Span,
) -> Result<String, CompileError> {
    if indent == 0 {
        return Ok(content.to_string());
    }
    let mut result = String::new();
    for (i, line) in content.split('\n').enumerate() {
        if i > 0 {
            result.push('\n');
        }
        let (body, carriage_return) = match line.strip_suffix('\r') {
            Some(stripped) => (stripped, "\r"),
            None => (line, ""),
        };
        let mut removed = 0;
        let mut rest = body;
        while removed < indent {
            match rest.chars().next() {
                Some(c) if c == ' ' || c == '\t' => {
                    rest = &rest[1..];
                    removed += 1;
                }
                _ => break,
            }
        }
        if removed < indent && !rest.is_empty() {
            return Err(CompileError::new(
                span,
                "Invalid heredoc body indentation level",
            ));
        }
        result.push_str(rest);
        result.push_str(carriage_return);
    }
    Ok(result)
}

/// Controls how a bare backslash at end of input is treated inside double-quoted strings.
#[derive(Clone, Copy)]
enum MissingEscape {
    /// Report an "Unterminated string literal" compile error.
    Error,
    /// Return the backslash as a literal `\` character.
    Literal,
}

/// Processes a double-quoted string escape sequence starting after the `\`.
/// Handles `\n`, `\r`, `\t`, `\v`, `\e`, `\f`, `\\`, `\"`, `\$`, `\x` hex,
/// `\u{...}` Unicode codepoint, and `\0`–`\7` octal escapes. On a bare `\`
/// at end of input, returns an error or a literal `\` depending on `missing_escape`.
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

/// Scans up to two hex digits after `\x` and appends the corresponding byte
/// to `out`. If no valid hex digit follows `\x`, appends `\x` literally.
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

/// Scans up to three octal digits starting from `first` (already consumed from
/// the input) and appends the resulting byte to `out`. Masks to `0xff` to handle
/// overflow silently (matching PHP behavior).
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

/// Scans a `\u{...}` Unicode codepoint escape after the `\u` has been consumed.
/// Validates the codepoint range (0x to 0x10ffff, excluding surrogates). Appends
/// the UTF-8 encoding of a valid codepoint to `out`, or returns an error.
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

/// Appends the UTF-8 encoding of `codepoint` as three bytes to `out`.
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

/// Appends a single escaped byte to `out` using the runtime escape helper.
fn push_byte_escape(byte: u8, out: &mut String) {
    crate::string_bytes::push_escaped_byte(byte, out);
}

/// Appends a single literal character to `out` using the runtime literal helper.
fn push_literal_char(ch: char, out: &mut String) {
    crate::string_bytes::push_literal_char(ch, out);
}
