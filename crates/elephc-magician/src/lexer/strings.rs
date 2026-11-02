//! Purpose:
//! Expands double-quoted eval-fragment string literals and heredoc bodies into
//! interpolation-aware token streams. A body without interpolation stays exactly one
//! `TokenKind::String`; a body with interpolation becomes a parenthesized `.` concatenation
//! the existing grammar already lowers to `EvalBinOp::Concat`.
//!
//! Called from:
//! - `super::scan::Lexer::next_tokens()` for every `"` and every `<<<` in a fragment.
//!
//! Key details:
//! - A heredoc body IS a double-quoted body in PHP, so both run through one scanner,
//!   `Lexer::lex_interpolated_body()`, and cannot drift apart.
//! - The two bodies differ in exactly two measured ways: a `"` ends a double-quoted literal
//!   but is ordinary text in a heredoc, and `\"` yields `"` in a double-quoted literal but
//!   keeps its backslash in a heredoc. `php -n` 8.5.6 prints `quote:\"` for a heredoc line
//!   reading `quote:\"`.
//! - A nowdoc body is a single-quoted body: no interpolation and no escape expansion.
//! - `\x`, octal and `\u{}` escapes can name a byte that is not valid UTF-8, so they are
//!   emitted as the shared literal markers (`elephc_builtin_contract::string_literal`, the
//!   ones the compiler's own lexer uses), which the parser decodes into `EvalConst::Bytes`.
//!   Source text that happens to sit in that marker range is re-encoded so it cannot collide.
//! - Every synthetic token carries the line of the opening quote or `<<<`, keeping
//!   `__LINE__` stable across multi-line literals.
//! - PHP simple syntax allows exactly one `[offset]` or `->prop` after `$name`; anything
//!   deeper requires the complex `{$expr}` form.
//! - Malformed offsets (`"$a[]"`, `"$a[ 0]"`, `"$a['k']"`) are refused, matching PHP's
//!   parse errors rather than silently inventing a key.

use super::scan::{is_ident_start, tokenize, Lexer};
use super::{Token, TokenKind};
use crate::errors::EvalParseError;
use elephc_builtin_contract::string_literal::{push_escaped_byte, push_literal_char};

/// Selects which of PHP's two interpolating string bodies is being scanned.
///
/// Both share one scanner because PHP defines a heredoc body as a double-quoted body; the
/// enum names the two places the reference lexer treats them differently.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum InterpolatedBody {
    /// A `"…"` literal, ending at its closing quote, where `\"` produces `"`.
    DoubleQuoted,
    /// A heredoc body already cut out at its closing marker, where `\"` stays `\"`.
    Heredoc,
}

impl Lexer<'_> {
    /// Reads a PHP heredoc or nowdoc starting at its `<<<`.
    ///
    /// The opener, the closing-marker search and PHP 7.3+ flexible indentation stripping are
    /// shared: they are the same in both forms. Only the body treatment differs. A nowdoc
    /// (`<<<'EOT'`) is a single-quoted body and becomes one literal token verbatim. A heredoc
    /// (`<<<EOT` or `<<<"EOT"`) is a double-quoted body, so the extracted text is handed to
    /// the same interpolation scanner a `"…"` literal uses and yields either one literal
    /// token or a parenthesized `.` concatenation.
    pub(super) fn lex_heredoc(&mut self, line: i64) -> Result<Vec<Token>, EvalParseError> {
        self.bump_char();
        self.bump_char();
        self.bump_char();
        while matches!(self.peek_char(), Some(' ' | '\t')) {
            self.bump_char();
        }
        let quote = match self.peek_char() {
            Some('\'') | Some('"') => self.peek_char(),
            _ => None,
        };
        if quote.is_some() {
            self.bump_char();
        }
        let label = self.lex_ident();
        if label.is_empty() || quote.is_some_and(|quote| self.peek_char() != Some(quote)) {
            return Err(EvalParseError::UnexpectedToken);
        }
        if quote.is_some() {
            self.bump_char();
        }
        if self.peek_char() == Some('\r') {
            self.bump_char();
        }
        if self.peek_char() != Some('\n') {
            return Err(EvalParseError::UnexpectedToken);
        }
        self.bump_char();

        let mut content = String::new();
        let mut at_line_start = true;
        loop {
            if self.peek_char().is_none() {
                return Err(EvalParseError::UnterminatedString);
            }
            if at_line_start {
                let remaining = self.remaining();
                let indent = remaining
                    .bytes()
                    .take_while(|byte| matches!(*byte, b' ' | b'\t'))
                    .count();
                let after_indent = &remaining[indent..];
                if after_indent.starts_with(&label) {
                    let after_label = &after_indent[label.len()..];
                    let closes = after_label
                        .chars()
                        .next()
                        .is_none_or(|ch| !is_ident_start(ch) && !ch.is_ascii_digit());
                    if closes {
                        // The indentation is space/tab only, so its char count equals its
                        // byte count; the label may hold multi-byte characters, which is why
                        // it is stepped over per character rather than per byte.
                        for _ in 0..indent {
                            self.bump_char();
                        }
                        for _ in label.chars() {
                            self.bump_char();
                        }
                        if content.ends_with('\n') {
                            content.pop();
                            if content.ends_with('\r') {
                                content.pop();
                            }
                        }
                        let content = strip_heredoc_indentation(&content, indent)?;
                        if quote == Some('\'') {
                            // Verbatim, but source text in the byte-marker range is
                            // re-encoded so the parser cannot mistake it for a marker.
                            let mut literal = String::with_capacity(content.len());
                            for ch in content.chars() {
                                push_literal_char(ch, &mut literal);
                            }
                            return Ok(vec![Token::new(TokenKind::String(literal), line)]);
                        }
                        return Lexer::new(&content)
                            .lex_interpolated_body(line, InterpolatedBody::Heredoc);
                    }
                }
            }
            let ch = self.peek_char().expect("EOF handled above");
            self.bump_char();
            at_line_start = ch == '\n';
            content.push(ch);
        }
    }

    /// Reads a double-quoted string literal starting at the opening quote.
    ///
    /// Returns exactly one `TokenKind::String` when the literal contains no
    /// interpolation, otherwise `( <part> . <part> ... )` as a token stream.
    pub(super) fn lex_double_quoted(&mut self, line: i64) -> Result<Vec<Token>, EvalParseError> {
        self.bump_char();
        self.lex_interpolated_body(line, InterpolatedBody::DoubleQuoted)
    }

    /// Scans one interpolating PHP string body from the cursor to its end.
    ///
    /// For `InterpolatedBody::DoubleQuoted` the opening quote is already consumed and the
    /// body ends at the closing quote, whose absence is an unterminated literal. For
    /// `InterpolatedBody::Heredoc` the lexer runs over a body already cut out at the closing
    /// marker, so the end of the input IS the end of the body and cannot be an error.
    ///
    /// Returns exactly one `TokenKind::String` when the body contains no interpolation,
    /// otherwise `( <part> . <part> ... )` as a token stream.
    fn lex_interpolated_body(
        &mut self,
        line: i64,
        body: InterpolatedBody,
    ) -> Result<Vec<Token>, EvalParseError> {
        let mut tokens: Vec<Token> = Vec::new();
        let mut current = String::new();
        let mut has_interpolation = false;
        // A heredoc body has no closing delimiter left to find: it was removed with the
        // marker line, so reaching the end of this input is the normal, terminated case.
        let mut terminated = body == InterpolatedBody::Heredoc;

        while let Some(ch) = self.peek_char() {
            if ch == '"' && body == InterpolatedBody::DoubleQuoted {
                self.bump_char();
                terminated = true;
                break;
            }
            match ch {
                '\\' => {
                    self.bump_char();
                    if self.peek_char().is_none() {
                        // `php -n` 8.5.6 keeps a heredoc body's final lone backslash as
                        // text; only a double-quoted literal can run out of input here.
                        if body == InterpolatedBody::Heredoc {
                            current.push('\\');
                            break;
                        }
                        return Err(EvalParseError::UnterminatedString);
                    }
                    self.push_escape(body, &mut current)?;
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

    /// Expands one escape sequence into the pending literal text, cursor on the character
    /// after the backslash, leaving it past the whole sequence.
    ///
    /// Every value here is `php -n` 8.5.6 through `bin2hex()`:
    /// `\x41`→`41`, `\x4`→`04`, `\x4z`→`047a`, `\x414`→`4134`, `\X41`→`41` (PHP accepts both
    /// letter cases), `\xZZ`→`5c785a5a` and `\x` at the end→`5c78` (no digit, so the
    /// backslash survives); `\101`→`41`, `\1`→`01`, `\1011`→`4131` (three digits at most),
    /// `\777`→`ff` and `\400`→`00` (PHP warns `Octal escape sequence overflow` and keeps the
    /// low byte), `\8`→`5c38`; `\u{41}`→`41`, `\u{1F600}`→`f09f9880`, `\u{0000041}`→`41`,
    /// `\u{10FFFF}`→`f48fbfbf`, `\u{D800}`→`eda080`, while `\u41`→`5c753431` and
    /// `\U{41}`→`5c557b34317d` stay text because only a lowercase `u` with a brace starts
    /// the sequence.
    ///
    /// The four malformed brace forms — `\u{}`, `\u{ZZ}`, `\u{41` and `\u{110000}` — are
    /// PHP parse errors (`Invalid UTF-8 codepoint escape sequence`), so they are refused
    /// rather than kept as text.
    fn push_escape(
        &mut self,
        body: InterpolatedBody,
        out: &mut String,
    ) -> Result<(), EvalParseError> {
        let escaped = self
            .peek_char()
            .expect("the caller rejects a backslash with nothing after it");
        match escaped {
            'x' | 'X' => {
                self.push_hex_escape(escaped, out);
                return Ok(());
            }
            '0'..='7' => {
                self.push_octal_escape(out);
                return Ok(());
            }
            'u' if self.peek_next_char() == Some('{') => return self.push_code_point_escape(out),
            _ => {}
        }
        self.bump_char();
        match escaped {
            'n' => out.push('\n'),
            'r' => out.push('\r'),
            't' => out.push('\t'),
            'v' => out.push('\x0b'),
            'e' => out.push('\x1b'),
            'f' => out.push('\x0c'),
            '\\' => out.push('\\'),
            // `\"` is the one entry that depends on the body. PHP prints `"` for the
            // double-quoted literal `"\""` and `\"` for a heredoc line reading `\"`,
            // because a heredoc has no quote to escape.
            '"' if body == InterpolatedBody::DoubleQuoted => out.push('"'),
            '$' => out.push('$'),
            other => {
                out.push('\\');
                out.push(other);
            }
        }
        Ok(())
    }

    /// Expands `\x41`-style escapes, cursor on the `x`, consuming at most two hex digits.
    ///
    /// With no hex digit at all the sequence is not an escape: PHP keeps `\xZZ` verbatim,
    /// so the backslash and the letter are pushed as text.
    fn push_hex_escape(&mut self, letter: char, out: &mut String) {
        let Some(first) = self.peek_next_char().and_then(|ch| ch.to_digit(16)) else {
            self.bump_char();
            out.push('\\');
            out.push(letter);
            return;
        };
        self.bump_char();
        self.bump_char();
        let mut value = first;
        if let Some(second) = self.peek_char().and_then(|ch| ch.to_digit(16)) {
            self.bump_char();
            value = value * 16 + second;
        }
        push_source_byte(value, out);
    }

    /// Expands `\101`-style escapes, cursor on the first octal digit, consuming at most three.
    ///
    /// PHP keeps only the low byte of an overflowing sequence (`\400` is `\0`) and warns at
    /// compile time; this scanner has no channel to a warning, so it produces the value only.
    fn push_octal_escape(&mut self, out: &mut String) {
        let mut value: u32 = 0;
        for _ in 0..3 {
            let Some(digit) = self.peek_char().and_then(|ch| ch.to_digit(8)) else {
                break;
            };
            value = value * 8 + digit;
            self.bump_char();
        }
        push_source_byte(value & 0xFF, out);
    }

    /// Expands `\u{1F600}`-style escapes, cursor on the `u`, consuming through the `}`.
    ///
    /// A malformed body is `EvalParseError::InvalidUtf8`, the kind that names PHP's own
    /// "Invalid UTF-8 codepoint escape sequence" parse error.
    fn push_code_point_escape(&mut self, out: &mut String) -> Result<(), EvalParseError> {
        self.bump_char();
        self.bump_char();
        let mut value: u32 = 0;
        let mut digits = 0usize;
        loop {
            let Some(ch) = self.peek_char() else {
                return Err(EvalParseError::InvalidUtf8);
            };
            if ch == '}' {
                self.bump_char();
                break;
            }
            let Some(digit) = ch.to_digit(16) else {
                return Err(EvalParseError::InvalidUtf8);
            };
            value = value * 16 + digit;
            if value > MAX_CODE_POINT {
                return Err(EvalParseError::InvalidUtf8);
            }
            self.bump_char();
            digits += 1;
        }
        if digits == 0 {
            return Err(EvalParseError::InvalidUtf8);
        }
        push_code_point(value, out);
        Ok(())
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
                '/' if self.peek_char() == Some('*') => {
                    inner.push('/');
                    self.bump_char();
                    self.capture_braced_block_comment(&mut inner)?;
                }
                '/' if self.peek_char() == Some('/') => {
                    // BOTH slashes are copied: the capture is lexed again as a whole, and a lone
                    // `/` there is a division — `"{$a[ // " }⏎"k"]}"` then read `" }…` as an
                    // unterminated string.
                    inner.push_str("//");
                    self.bump_char();
                    self.capture_braced_line_comment(&mut inner)?;
                }
                '#' if self.peek_char() != Some('[') => {
                    inner.push('#');
                    self.capture_braced_line_comment(&mut inner)?;
                }
                other => inner.push(other),
            }
        }
    }

    /// Copies a block comment verbatim so its braces and quotes do not affect nesting.
    ///
    /// The caller has already consumed the opening `/*`; only the `*` is still to be copied.
    /// This used to consume one more character as well, which dropped the first byte of the
    /// body: harmless for `/*x*/`, fatal for `/**/`, whose first body byte is the closing star,
    /// so the scan ran past `*/` and `"{$a[/**/"k"]}"` was "eval() fragment is invalid"
    /// (MEASURED on PHP 8.5.10: reference prints `v`).
    fn capture_braced_block_comment(&mut self, inner: &mut String) -> Result<(), EvalParseError> {
        inner.push('*');
        while let Some(ch) = self.peek_char() {
            self.bump_char();
            inner.push(ch);
            if ch == '*' && self.peek_char() == Some('/') {
                inner.push('/');
                self.bump_char();
                return Ok(());
            }
        }
        Err(EvalParseError::UnterminatedString)
    }

    /// Copies a line comment through its newline, leaving braces and quotes inert.
    ///
    /// A `?>` ends a PHP line comment too, and inside `{$…}` it then ends PHP mode in the middle
    /// of the expression, which reference PHP always rejects. The copy used to run to the
    /// newline, hiding the `?>`: `"{$a[// ?>⏎"k"]}"` evaluated to `v` in eval'd code, where
    /// PHP 8.5.10 raises a parse error (MEASURED; reported by two reviewers of PR #968). The
    /// capture refuses it itself rather than stopping before it, because the captured text is
    /// lexed again, and that pass's line-comment skip would swallow the `?>` a second time.
    fn capture_braced_line_comment(&mut self, inner: &mut String) -> Result<(), EvalParseError> {
        while let Some(ch) = self.peek_char() {
            if ch == '?' && self.peek_next_char() == Some('>') {
                return Err(EvalParseError::UnexpectedToken);
            }
            self.bump_char();
            inner.push(ch);
            if ch == '\n' {
                break;
            }
        }
        Ok(())
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
}

/// Removes flexible heredoc/nowdoc indentation using PHP's closing-marker width rule.
fn strip_heredoc_indentation(content: &str, indent: usize) -> Result<String, EvalParseError> {
    if indent == 0 {
        return Ok(content.to_owned());
    }
    let mut stripped = String::new();
    for (index, line) in content.split('\n').enumerate() {
        if index > 0 {
            stripped.push('\n');
        }
        let (body, carriage_return) = line
            .strip_suffix('\r')
            .map_or((line, ""), |body| (body, "\r"));
        let removed = body
            .bytes()
            .take_while(|byte| matches!(*byte, b' ' | b'\t'))
            .take(indent)
            .count();
        if removed < indent && !body.is_empty() {
            return Err(EvalParseError::UnexpectedToken);
        }
        stripped.push_str(&body[removed..]);
        stripped.push_str(carriage_return);
    }
    Ok(stripped)
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
            .filter(|token| !matches!(token.kind(), TokenKind::Eof | TokenKind::DocComment(_)))
            .map(|token| Token::new(token.into_kind(), line)),
    );
    part.push(Token::new(TokenKind::RParen, line));
    Ok(part)
}

/// The largest code point `\u{…}` accepts, matching PHP's `Codepoint too large` boundary.
const MAX_CODE_POINT: u32 = 0x10_FFFF;

/// Appends one raw source byte produced by a `\x` or octal escape to the pending text.
///
/// A PHP string is a byte sequence and `"\xFF"` is the single byte `0xFF`, which is not
/// valid UTF-8 and so cannot live in a Rust `String` as itself. Bytes at or above `0x80`
/// are therefore written as the shared literal markers (`push_escaped_byte`), the ones the
/// compiler's lexer writes for the same escape; `crate::parser` decodes that range back into
/// `EvalConst::Bytes`, so the literal reaches the interpreter as the byte PHP produced.
fn push_source_byte(byte: u32, out: &mut String) {
    let byte = u8::try_from(byte).expect("escape scanners mask their value to one byte");
    push_escaped_byte(byte, out);
}

/// Appends the UTF-8 encoding of one `\u{…}` code point to the pending text.
///
/// PHP encodes a lone surrogate rather than refusing it: `bin2hex("\u{D800}")` is `eda080`.
/// Rust's `char` cannot hold a surrogate, so those three bytes are emitted through the same
/// byte markers a `\x` escape uses and the literal becomes `EvalConst::Bytes`, which is what
/// PHP hands the program too.
fn push_code_point(value: u32, out: &mut String) {
    if let Some(ch) = char::from_u32(value) {
        // Through `push_literal_char`: `"\u{E000}"` is the three bytes `ee8080` in PHP, and
        // pushed as itself the character would read back as the byte marker for `0x00`.
        push_literal_char(ch, out);
        return;
    }
    push_source_byte(0xE0 | (value >> 12), out);
    push_source_byte(0x80 | ((value >> 6) & 0x3F), out);
    push_source_byte(0x80 | (value & 0x3F), out);
}
