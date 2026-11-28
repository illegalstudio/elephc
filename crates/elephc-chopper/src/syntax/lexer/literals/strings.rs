//! Purpose:
//! Scans PHP string literal forms, including interpolation-aware double strings and heredocs.
//! Emits string tokens plus interpolation boundary tokens that the parser can consume.
//!
//! Called from:
//! - `crate::syntax::lexer::scan` through `crate::syntax::lexer::literals`.
//!
//! Key details:
//! - Escape and interpolation behavior must preserve PHP-compatible text and source spans.

use super::super::cursor::Cursor;
use super::super::token::{spanned, SpannedToken, Token};
use super::identifiers::is_ident_continue;
use crate::syntax::errors::CompileError;
use crate::syntax::parser::validation_only;
use crate::syntax::span::Span;
use crate::scanner::{
    braced_expression_len, braced_interpolation, decode_single_quoted, heredoc_closing,
    heredoc_opening, BracedInterpolation, HeredocOpeningError, HeredocProfile,
    InterpolationComment, InterpolationError,
};

/// Presents physical input and de-indented heredoc text to the same string decoder.
trait EscapeInput {
    /// Returns the raw source tail at the current character boundary.
    fn remaining(&self) -> &str;
    /// Advances past a known source range that ends at a character boundary.
    fn skip_bytes(&mut self, len: usize);
    /// Returns the next character without consuming it.
    fn peek_escape(&self) -> Option<char> {
        self.remaining().chars().next()
    }
    /// Consumes and returns the next character.
    fn advance_escape(&mut self) -> Option<char>;
    /// Returns the character `n` positions ahead (0 = next) without consuming it.
    fn peek_nth(&self, n: usize) -> Option<char> {
        self.remaining().chars().nth(n)
    }
}

impl EscapeInput for Cursor<'_> {
    /// Returns the remaining bytes of the physical input.
    fn remaining(&self) -> &str {
        Cursor::remaining(self)
    }
    /// Consumes a known byte range and keeps physical source coordinates.
    fn skip_bytes(&mut self, len: usize) {
        Cursor::skip_bytes(self, len);
    }
    /// Returns the next character without advancing the physical cursor.
    fn peek_escape(&self) -> Option<char> {
        self.peek()
    }
    /// Consumes the next character and updates physical source coordinates.
    fn advance_escape(&mut self) -> Option<char> {
        self.advance()
    }
}

/// Holds the de-indented body independently of the physical source cursor.
struct StringInput<'a>(&'a str);

impl EscapeInput for StringInput<'_> {
    /// Returns the remaining de-indented text.
    fn remaining(&self) -> &str {
        self.0
    }
    /// Advances through a known range in the de-indented text.
    fn skip_bytes(&mut self, len: usize) {
        self.0 = &self.0[len..];
    }
    /// Consumes the next character of the de-indented text.
    fn advance_escape(&mut self) -> Option<char> {
        let mut chars = self.0.chars();
        let ch = chars.next()?;
        self.0 = chars.as_str();
        Some(ch)
    }
}

/// Decodes a double-quoted body after the shared scanner recognizes its opening quote.
/// Returns one or more tokens: for `"Hello $name!"` it returns
/// `("Hello " . Variable("name") . "!")` (parenthesized, with Dot tokens for concatenation).
pub(crate) fn decode_double_quoted_body(
    cursor: &mut Cursor,
    span: Span,
) -> Result<Vec<SpannedToken>, CompileError> {
    decode_quoted_interpolation(cursor, span, '"')
}

/// Shares the semantic profile of the enclosing lexer with quoted interpolation.
fn decode_quoted_interpolation(
    cursor: &mut Cursor,
    span: Span,
    delimiter: char,
) -> Result<Vec<SpannedToken>, CompileError> {
    let profile = cursor.semantic_profile;
    interpolate(cursor, span, Some(delimiter), MissingEscape::Error, profile)
}

/// Appends one interpolated expression part (already a token list) to the running stream,
/// flushing any pending literal text and inserting `.` concatenation.
///
/// When this is the first part, the pending literal (possibly empty) is emitted first so
/// the resulting `.` chain is always string-typed, matching PHP's rule that a
/// double-quoted/heredoc string is always a string.
fn push_interp_part(
    tokens: &mut Vec<SpannedToken>,
    current: &mut String,
    part: Vec<SpannedToken>,
    span: Span,
) {
    if tokens.is_empty() {
        tokens.push(spanned(
            Token::StringLiteral(std::mem::take(current)),
            span,
        ));
    } else if !current.is_empty() {
        tokens.push(spanned(Token::Dot, span));
        tokens.push(spanned(
            Token::StringLiteral(std::mem::take(current)),
            span,
        ));
    }
    tokens.push(spanned(Token::Dot, span));
    tokens.push(spanned(Token::StringFormat, span));
    tokens.push(spanned(Token::LParen, span));
    for token in part {
        tokens.push(token);
    }
    tokens.push(spanned(Token::RParen, span));
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
    profile: elephc_semantics::SemanticProfile,
) -> Result<Vec<SpannedToken>, CompileError> {
    let mut tokens: Vec<SpannedToken> = Vec::new();
    let mut current = String::new();
    let mut has_interpolation = false;

    loop {
        match input.peek_escape() {
            None => {
                if let Some(terminator) = terminator {
                    if validation_only() {
                        let message = if terminator == '`' {
                            "syntax error, unexpected end of file, expecting \"`\""
                        } else {
                            "syntax error, unexpected end of file, expecting variable or \"${\" or \"{$\""
                        };
                        return Err(CompileError::new(span, message));
                    }
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
            Some('{') if braced_interpolation(input.remaining().as_bytes()) == Some(BracedInterpolation::CurlyDollar) => {
                input.advance_escape(); // consume '{'
                let inner = capture_braced_expr(input, span)?;
                let fragment = tokenize_fragment(&inner, span, profile)?;
                has_interpolation = true;
                let mut part = vec![spanned(Token::LParen, span)];
                part.extend(fragment);
                part.push(spanned(Token::RParen, span));
                push_interp_part(&mut tokens, &mut current, part, span);
            }
            Some('$') => {
                // Deprecated ${expr} form: dollar followed by brace. PHP 8.2+ deprecates
                // this in favor of {$expr} but still supports it. We capture the braced
                // expression (without the leading $) and interpolate it.
                if braced_interpolation(input.remaining().as_bytes()) == Some(BracedInterpolation::DollarCurly) {
                    input.advance_escape(); // consume '$'
                    input.advance_escape(); // consume '{'
                    let inner_raw = capture_braced_expr(input, span)?;
                    // Re-prepend the $ so the expression is valid PHP ($a['x'] not a['x'])
                    let inner = format!("${}", inner_raw);
                    let fragment = tokenize_fragment(&inner, span, profile)?;
                    has_interpolation = true;
                    let mut part = vec![spanned(Token::LParen, span)];
                    part.extend(fragment);
                    part.push(spanned(Token::RParen, span));
                    push_interp_part(&mut tokens, &mut current, part, span);
                    continue;
                }
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
                    let mut part = vec![spanned(Token::Variable(name), span)];
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
        return Ok(vec![spanned(Token::StringLiteral(current), span)]);
    }

    if !current.is_empty() {
        tokens.push(spanned(Token::Dot, span));
        tokens.push(spanned(Token::StringLiteral(current), span));
    }

    let mut result = vec![spanned(Token::LParen, span)];
    result.extend(tokens);
    result.push(spanned(Token::RParen, span));
    Ok(result)
}

/// Appends the simple-syntax access that may follow a `$name` in an interpolated string:
/// a single `[offset]` or a single `->prop`. PHP's "simple syntax" allows exactly one
/// level; anything more must use the complex `{$expr}` form. Leaves `input` positioned
/// after the consumed access (or unchanged if none applies).
fn append_simple_access(
    input: &mut impl EscapeInput,
    part: &mut Vec<SpannedToken>,
    span: Span,
) -> Result<(), CompileError> {
    if input.peek_escape() == Some('[') {
        input.advance_escape(); // consume '['
        append_simple_offset_key(input, part, span)?;
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
        part.push(spanned(Token::Arrow, span));
        part.push(spanned(Token::Identifier(prop), span));
    }
    Ok(())
}

/// Reads the offset key for a simple `$name[offset]` interpolation and appends the
/// `[ key ]` tokens to `part`. PHP simple syntax keys are a `$var`, an optionally-negative
/// integer, or a bareword treated as a string key (never quoted).
fn append_simple_offset_key(
    input: &mut impl EscapeInput,
    part: &mut Vec<SpannedToken>,
    span: Span,
) -> Result<(), CompileError> {
    part.push(spanned(Token::LBracket, span));
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
            part.push(spanned(Token::Variable(name), span));
        }
        Some(c) if c == '-' || c.is_ascii_digit() => {
            let Some((len, key)) = elephc_semantics::array_keys::scan_simple_interpolation_numeric_key(
                input.remaining().as_bytes(),
            ) else {
                return Err(CompileError::new(
                    span,
                    "Expected digits after '-' in string interpolation offset",
                ));
            };
            let token = match key {
                elephc_semantics::array_keys::StringKey::Integer(value) => Token::IntLiteral(value),
                elephc_semantics::array_keys::StringKey::String(value) => Token::StringLiteral(
                    std::str::from_utf8(value)
                        .expect("interpolation numeric spellings are ASCII")
                        .to_owned(),
                ),
            };
            input.skip_bytes(len);
            // PHP's simple interpolation grammar canonicalizes integer-looking offsets
            // before ordinary array-key policy applies. Exact-key mode therefore keeps
            // noncanonical spellings but must still treat `1` as integer key 1.
            part.push(spanned(token, span));
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
            if key.is_empty() {
                return Err(CompileError::new(
                    span,
                    "Expected an array offset in string interpolation",
                ));
            }
            part.push(spanned(Token::StringLiteral(key), span));
        }
    }
    part.push(spanned(Token::RBracket, span));
    Ok(())
}

/// Captures a braced interpolation expression verbatim and consumes its closing brace.
fn capture_braced_expr(
    input: &mut impl EscapeInput,
    span: Span,
) -> Result<String, CompileError> {
    let source = input.remaining();
    let len = braced_expression_len(source.as_bytes(), InterpolationComment::EndAtCloseTag)
        .map_err(|error| {
            let message = match error {
                InterpolationError::Quote => "Unterminated string in complex interpolation '{$...}'",
                InterpolationError::BlockComment => "Unterminated comment in complex interpolation '{$...}'",
                InterpolationError::Expression | InterpolationError::CloseTag => {
                    "Unterminated complex interpolation '{$...}'"
                }
            };
            CompileError::new(span, message)
        })?;
    let inner = source[..len - 1].to_owned();
    input.skip_bytes(len);
    Ok(inner)
}

thread_local! {
    /// Current recursive `{$...}` fragment depth on this thread.
    static FRAGMENT_DEPTH: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Deepest nested `{$...}` fragment the lexer expands before reporting a syntax error.
const MAX_FRAGMENT_NESTING: usize = 256;

/// Decrements the fragment depth when a recursive lexer call returns.
struct FragmentDepthGuard;

impl Drop for FragmentDepthGuard {
    /// Restores the previous fragment depth, including on the error path.
    fn drop(&mut self) {
        FRAGMENT_DEPTH.with(|depth| depth.set(depth.get().saturating_sub(1)));
    }
}

/// Tokenizes the captured `{$expr}` source as a standalone expression by lexing it behind
/// a synthetic `<?php` tag, then dropping the open tag and EOF and re-spanning the tokens
/// to the enclosing string's span. Reuses the full lexer so nested strings, calls, and
/// array/property access inside the braces are handled like any other expression.
fn tokenize_fragment(
    inner: &str,
    span: Span,
    profile: elephc_semantics::SemanticProfile,
) -> Result<Vec<SpannedToken>, CompileError> {
    let depth = FRAGMENT_DEPTH.with(|depth| {
        let current = depth.get();
        depth.set(current + 1);
        current
    });
    let _guard = FragmentDepthGuard;
    if depth >= MAX_FRAGMENT_NESTING {
        return Err(CompileError::new(
            span,
            "Maximum interpolation nesting level exceeded",
        ));
    }
    let source = format!("<?php {}", inner);
    let tokens = crate::syntax::lexer::tokenize_with_semantics(
        &source,
        crate::syntax::source::SourceMode::Php,
        span.source_id(),
        profile,
    )?;
    Ok(tokens
        .into_iter()
        .filter(|(token, _)| !matches!(token, Token::OpenTag | Token::Eof))
        .map(|(token, metadata)| (token, metadata.with_span(span)))
        .collect())
}

/// Decodes a backtick shell-string body after the opening backtick is consumed.
pub(crate) fn decode_backtick_body(
    cursor: &mut Cursor,
    span: Span,
) -> Result<Vec<SpannedToken>, CompileError> {
    decode_quoted_interpolation(cursor, span, '`')
}

/// Decodes one single-quoted lexeme or diagnoses a missing closing quote.
pub(crate) fn single_quoted_token(
    text: &str,
    terminated: bool,
    span: Span,
) -> Result<Token, CompileError> {
    let raw_content = &text[1..];
    if !terminated {
        if validation_only() {
            let content = raw_content.lines().next().unwrap_or("");
            return Err(CompileError::new(
                span,
                &format!("syntax error, unexpected string content \"{content}\", expecting end of file"),
            ));
        }
        return Err(CompileError::new(span, "Unterminated string literal"));
    }
    Ok(Token::StringLiteral(decode_single_quoted(&raw_content[..raw_content.len() - 1])))
}

/// Scan a heredoc or nowdoc string.
/// At this point, `<<<` has already been consumed.
/// Heredoc: `<<<LABEL` or `<<<"LABEL"` — supports variable interpolation like double-quoted strings
/// Nowdoc: `<<<'LABEL'` — no interpolation (like single-quoted strings)
pub(crate) fn scan_heredoc(
    cursor: &mut Cursor,
) -> Result<Vec<SpannedToken>, CompileError> {
    let span = cursor.span();

    let opening = heredoc_opening(cursor.remaining().as_bytes(), HeredocProfile::Compiler)
        .map_err(|error| {
            let message = match error {
                HeredocOpeningError::Label => "Expected heredoc/nowdoc label after '<<<'",
                HeredocOpeningError::Quote(b'\'') => "Expected closing ' for nowdoc label",
                HeredocOpeningError::Quote(_) => "Expected closing \" for heredoc label",
                HeredocOpeningError::Newline => "Expected newline after heredoc/nowdoc label",
            };
            CompileError::new(span, message)
        })?;
    cursor.skip_bytes(opening.len);

    let mut content = String::new();
    let mut at_line_start = true;
    loop {
        if cursor.is_eof() {
            return Err(CompileError::new(span, "Unterminated heredoc/nowdoc"));
        }

        // The closing label is only recognized at the start of a line (optionally indented),
        // so a label appearing mid-line is treated as body content.
        if at_line_start {
            if let Some((len, indent)) = heredoc_closing(
                cursor.remaining().as_bytes(),
                opening.label,
                HeredocProfile::Compiler,
            ) {
                cursor.skip_bytes(len);
                if content.ends_with('\n') {
                    content.pop();
                    if content.ends_with('\r') {
                        content.pop();
                    }
                }

                let content = strip_heredoc_indentation(&content, indent, opening.nowdoc, span)?;
                if opening.nowdoc {
                    return Ok(vec![spanned(Token::StringLiteral(content), span)]);
                }
                return interpolate(
                    &mut StringInput(&content),
                    span,
                    None,
                    MissingEscape::Literal,
                    cursor.semantic_profile,
                );
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
    is_nowdoc: bool,
    span: Span,
) -> Result<String, CompileError> {
    if indent == 0 {
        return Ok(content.to_string());
    }
    // Zend only checks the lines that begin in literal text: a newline inside `{$...}` or
    // `${...}` does not start a heredoc body line, so those lines pass through unchanged.
    // A nowdoc has no interpolation at all, so every line is literal.
    let expressions = if is_nowdoc {
        Vec::new()
    } else {
        interpolation_ranges(content)
    };
    let mut result = String::new();
    let mut offset = 0usize;
    let mut first = true;
    let mut range_index = 0usize;
    for line in content.split('\n') {
        if !first {
            result.push('\n');
            offset += 1;
        }
        first = false;
        while range_index < expressions.len() && offset >= expressions[range_index].1 {
            range_index += 1;
        }
        let in_expression = expressions
            .get(range_index)
            .is_some_and(|&(start, end)| offset >= start && offset < end);
        let (body, carriage_return) = match line.strip_suffix('\r') {
            Some(stripped) => (stripped, "\r"),
            None => (line, ""),
        };
        if in_expression {
            result.push_str(line);
            offset += line.len();
            continue;
        }
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
        offset += line.len();
    }
    Ok(result)
}

/// Returns whether an odd run of backslashes directly precedes `index`.
fn escaped_by_backslash(bytes: &[u8], index: usize) -> bool {
    let mut cursor = index;
    let mut backslashes = 0usize;
    while cursor > 0 && bytes[cursor - 1] == b'\\' {
        backslashes += 1;
        cursor -= 1;
    }
    backslashes % 2 == 1
}

/// Returns the byte ranges of `{$...}` and `${...}` expressions in one heredoc body.
///
/// The scan only needs to know where an expression starts and ends; quoted strings inside it
/// are skipped so a brace in a string literal cannot unbalance the range.
fn interpolation_ranges(content: &str) -> Vec<(usize, usize)> {
    let bytes = content.as_bytes();
    let mut ranges = Vec::new();
    let mut index = 0usize;
    while index < bytes.len() {
        if braced_interpolation(&bytes[index..]).is_none() || escaped_by_backslash(bytes, index) {
            index += 1;
            continue;
        }
        let start = index;
        index += 2;
        index += braced_expression_len(&bytes[index..], InterpolationComment::UntilNewline)
            .unwrap_or(bytes.len() - index);
        ranges.push((start, index));
    }
    ranges
}

#[cfg(test)]
mod tests {
    use super::Token;

    /// A lone minus is not an integer offset and must not become the string key `"-"`.
    #[test]
    fn simple_interpolation_rejects_lone_minus_offset() {
        assert!(crate::syntax::lexer::tokenize("<?php echo \"$items[-]\";").is_err());
    }

    /// An empty offset is invalid PHP syntax and must not become the empty-string key.
    #[test]
    fn simple_interpolation_rejects_empty_offset() {
        assert!(crate::syntax::lexer::tokenize("<?php echo \"$items[]\";").is_err());
    }

    /// Numeric-looking offset text keeps PHP's integer-versus-string distinction.
    #[test]
    fn simple_interpolation_preserves_numeric_key_spellings() {
        for spelling in ["-9223372036854775808", "0x10", "-0x10", "0b10", "0o10", "1_0"] {
            let tokens = crate::syntax::lexer::tokenize(&format!(
                "<?php echo \"$items[{spelling}]\";"
            ))
            .expect("numeric-looking interpolation offset should tokenize");
            assert!(tokens.iter().any(|(token, _)| {
                matches!(token, Token::StringLiteral(value) if value == spelling)
            }));
        }
        let tokens = crate::syntax::lexer::tokenize(
            "<?php echo \"$items[-9223372036854775807]\";",
        )
        .expect("in-range negative interpolation offset should tokenize");
        assert!(tokens.iter().any(|(token, _)| {
            *token == Token::IntLiteral(-9_223_372_036_854_775_807)
        }));
        for spelling in ["0x", "0b", "0o", "1_"] {
            assert!(
                crate::syntax::lexer::tokenize(&format!(
                    "<?php echo \"$items[{spelling}]\";"
                ))
                .is_err(),
                "{spelling}"
            );
        }
    }
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
    crate::syntax::string_bytes::push_escaped_byte(byte, out);
}

/// Appends a single literal character to `out` using the runtime literal helper.
fn push_literal_char(ch: char, out: &mut String) {
    crate::syntax::string_bytes::push_literal_char(ch, out);
}
