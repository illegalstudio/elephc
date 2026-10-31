//! Purpose:
//! Implements Zend-compatible byte tokenization without an external PHP grammar dependency.
//!
//! Called from:
//! - `crate::tokenize` before optional TOKEN_PARSE validation.
//!
//! Key details:
//! - Every source byte belongs to exactly one token; offsets, lengths and Zend lines are exact.
//! - Inline HTML, open/close tags, heredoc/nowdoc bodies and interpolation retain Zend boundaries.
//! - Malformed input remains tokenizable without TOKEN_PARSE; lexical errors are reported separately.

use crate::keywords::{
    cast_id, is_label, is_label_start, is_whitespace, keyword_id, operator_id,
};
use crate::{id, newline_count, Token};

/// One lexical diagnostic that TOKEN_PARSE must raise as a ParseError.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LexError {
    /// Byte offset of the offending token or character.
    pub pos: usize,
    /// PHP's exact parser-facing message.
    pub message: String,
}

/// One non-fatal TOKEN_PARSE diagnostic raised while scanning.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LexWarning {
    /// PHP's exact parser-facing message.
    pub message: String,
    /// Byte offset of the cast.
    pub pos: usize,
}

/// Scanner modes. `Php` is shared by top-level code and interpolation expressions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    InlineHtml,
    Php,
    DoubleQuote,
    Heredoc,
    Backtick,
}

/// One suspended string interpolation frame.
#[derive(Clone, Debug)]
struct Frame {
    mode: Mode,
    label: Vec<u8>,
    nowdoc: bool,
    depth: usize,
}

/// The scanner state over one immutable source buffer.
pub(crate) struct Lexer<'a> {
    source: &'a [u8],
    i: usize,
    line: usize,
    mode: Mode,
    property: bool,
    depth: usize,
    frames: Vec<Frame>,
    heredoc_label: Vec<u8>,
    nowdoc: bool,
    heredoc_at_line: bool,
    halt: Option<u8>,
    halt_done: bool,
    tokens: Vec<Token>,
    errors: Vec<LexError>,
    warnings: Vec<LexWarning>,
}

impl<'a> Lexer<'a> {
    /// Creates a scanner for one source buffer.
    fn new(source: &'a [u8]) -> Self {
        Self {
            source,
            i: 0,
            line: 1,
            mode: Mode::InlineHtml,
            property: false,
            depth: 0,
            frames: Vec::new(),
            heredoc_label: Vec::new(),
            nowdoc: false,
            heredoc_at_line: false,
            halt: None,
            halt_done: false,
            tokens: Vec::new(),
            errors: Vec::new(),
            warnings: Vec::new(),
        }
    }

    /// Runs the scanner to completion and returns its tokens and lexical diagnostics.
    fn run(mut self) -> (Vec<Token>, Vec<LexError>, Vec<LexWarning>) {
        while self.i < self.source.len() {
            if self.halt_done {
                let start = self.i;
                self.i = self.source.len();
                self.push(id("T_INLINE_HTML"), start, self.i);
                break;
            }
            let before = self.i;
            match self.mode {
                Mode::InlineHtml => self.scan_inline_html(),
                Mode::Php => self.scan_php(),
                Mode::DoubleQuote | Mode::Heredoc | Mode::Backtick => self.scan_string_body(),
            }
            if self.i <= before {
                self.i = before + 1;
            }
        }
        self.validate_heredocs();
        self.validate_halt_compiler();
        (self.tokens, self.errors, self.warnings)
    }

    /// Validates `__halt_compiler ( ) ;` for TOKEN_PARSE.
    fn validate_halt_compiler(&mut self) {
        let Some(halt_index) = self.tokens.iter().position(|token| token.id == id("T_HALT_COMPILER")) else {
            return;
        };
        let significant: Vec<&Token> = self.tokens[halt_index + 1..]
            .iter()
            .filter(|token| !is_trivia(token.id) && token.id != id("T_INLINE_HTML"))
            .collect();
        let text = |token: &Token| &self.source[token.pos..token.pos + token.len];
        let Some(open) = significant.first() else {
            self.errors.push(LexError {
                pos: self.source.len(),
                message: "syntax error, unexpected end of file, expecting \"(\"".into(),
            });
            return;
        };
        if text(open) != b"(" {
            self.errors.push(LexError {
                pos: open.pos,
                message: unexpected_expecting(self.source, open, "\"(\""),
            });
            return;
        }
        let Some(close) = significant.get(1) else {
            self.errors.push(LexError {
                pos: self.source.len(),
                message: "Unclosed '('".into(),
            });
            return;
        };
        if text(close) != b")" {
            self.errors.push(LexError {
                pos: close.pos,
                message: unexpected_expecting(self.source, close, "\")\"") ,
            });
            return;
        }
        let Some(semicolon) = significant.get(2) else {
            self.errors.push(LexError {
                pos: self.source.len(),
                message: "syntax error, unexpected end of file, expecting \";\"".into(),
            });
            return;
        };
        if text(semicolon) != b";" {
            self.errors.push(LexError {
                pos: semicolon.pos,
                message: unexpected_expecting(self.source, semicolon, "\";\""),
            });
        }
    }

    /// Validates flexible heredoc indentation and unterminated bodies for TOKEN_PARSE.
    fn validate_heredocs(&mut self) {
        let mut stack = Vec::new();
        for index in 0..self.tokens.len() {
            if self.tokens[index].id == id("T_START_HEREDOC") {
                stack.push(index);
            } else if self.tokens[index].id == id("T_END_HEREDOC") {
                if let Some(start) = stack.pop() {
                    self.validate_heredoc_pair(start, index);
                }
            }
        }
        if !stack.is_empty() {
            self.errors.push(LexError {
                pos: self.source.len(),
                message: "syntax error, unexpected end of file, expecting variable or heredoc end or \"${\" or \"{$\"".into(),
            });
        }
    }

    /// Checks one heredoc/nowdoc body against its closing label indentation.
    fn validate_heredoc_pair(&mut self, start_index: usize, end_index: usize) {
        let start = &self.tokens[start_index];
        let end = &self.tokens[end_index];
        let indent_text = &self.source[end.pos..end.pos + end.len];
        let indent_len = indent_text.iter().take_while(|b| matches!(b, b' ' | b'\t')).count();
        let indent = &indent_text[..indent_len];
        let body_start = start.pos + start.len;
        if indent.contains(&b' ') && indent.contains(&b'\t') {
            self.errors.push(LexError {
                pos: body_start,
                message: "Invalid indentation - tabs and spaces cannot be mixed".into(),
            });
            return;
        }
        if indent.is_empty() || body_start >= end.pos {
            return;
        }
        let body = &self.source[body_start..end.pos];
        let mut index = 0;
        while index < body.len() {
            let line_start = body_start + index;
            let mut line_end = index;
            while line_end < body.len() && !matches!(body[line_end], b'\n' | b'\r') {
                line_end += 1;
            }
            let content = &body[index..line_end];
            let leading = content.iter().take_while(|b| matches!(b, b' ' | b'\t')).count();
            let rest = &content[leading..];
            if !rest.is_empty() {
                if content[..leading].contains(&b' ') && content[..leading].contains(&b'\t') {
                    self.errors.push(LexError {
                        pos: line_start,
                        message: "Invalid indentation - tabs and spaces cannot be mixed".into(),
                    });
                    return;
                }
                let matching = content.iter().take_while(|b| **b == indent[0]).count();
                if matching < indent.len() {
                    self.errors.push(LexError {
                        pos: line_start,
                        message: format!(
                            "Invalid body indentation level (expecting an indentation level of at least {})",
                            indent.len()
                        ),
                    });
                    return;
                }
            }
            index = line_end;
            if index < body.len() {
                if body[index] == b'\r' && body.get(index + 1) == Some(&b'\n') {
                    index += 2;
                } else {
                    index += 1;
                }
            }
        }
    }

    /// Appends one token and advances the Zend line counter.
    fn push(&mut self, token_id: i64, start: usize, end: usize) {
        if start > end {
            return;
        }
        self.tokens.push(Token {
            id: token_id,
            pos: start,
            len: end - start,
            line: self.line,
        });
        self.line += newline_count(&self.source[start..end]);
        if self.halt.is_some() && !is_trivia(token_id) {
            let remaining = self.halt.unwrap_or(0);
            if remaining <= 1 {
                self.halt = None;
                self.halt_done = true;
            } else {
                self.halt = Some(remaining - 1);
            }
        }
    }

    /// Emits one single-byte punctuation token carrying the byte's numeric value.
    fn push_byte(&mut self, start: usize) {
        let byte = self.source[start];
        self.push(i64::from(byte), start, start + 1);
    }

    /// Scans inline HTML up to the next open tag, emitting one coalesced T_INLINE_HTML.
    fn scan_inline_html(&mut self) {
        let start = self.i;
        let mut probe = self.i;
        let mut tag = None;
        while probe + 1 < self.source.len() {
            let Some(offset) = find(&self.source[probe..], b"<?") else {
                break;
            };
            let at = probe + offset;
            if at + 5 <= self.source.len()
                && self.source[at + 2..at + 5].eq_ignore_ascii_case(b"php")
                && (at + 5 == self.source.len() || is_whitespace(self.source[at + 5]))
            {
                tag = Some((at, Mode::Php));
                break;
            }
            if at + 2 < self.source.len() && self.source[at + 2] == b'=' {
                tag = Some((at, Mode::Php));
                break;
            }
            probe = at + 2;
        }
        let Some((at, _)) = tag else {
            self.i = self.source.len();
            self.push(id("T_INLINE_HTML"), start, self.i);
            return;
        };
        if at > start {
            self.push(id("T_INLINE_HTML"), start, at);
        }
        if self.source[at + 2] == b'=' {
            self.push(id("T_OPEN_TAG_WITH_ECHO"), at, at + 3);
            self.i = at + 3;
            self.mode = Mode::Php;
            return;
        }
        let mut end = at + 5;
        if end < self.source.len() {
            if self.source[end] == b'\r' && self.source.get(end + 1) == Some(&b'\n') {
                end += 2;
            } else if is_whitespace(self.source[end]) {
                end += 1;
            }
        }
        self.push(id("T_OPEN_TAG"), at, end);
        self.i = end;
        self.mode = Mode::Php;
    }

    /// Scans one PHP-mode token (or starts a string body).
    fn scan_php(&mut self) {
        let c = self.source[self.i];
        let was_property = self.property;
        self.property = false;

        if is_whitespace(c) {
            let start = self.i;
            let mut j = self.i;
            while j < self.source.len() && is_whitespace(self.source[j]) {
                j += 1;
            }
            self.push(id("T_WHITESPACE"), start, j);
            self.i = j;
            self.property = was_property;
            return;
        }

        if c == b'?' && self.source.get(self.i + 1) == Some(&b'>') {
            let start = self.i;
            let mut end = self.i + 2;
            if self.source.get(end) == Some(&b'\r') && self.source.get(end + 1) == Some(&b'\n') {
                end += 2;
            } else if self.source.get(end).is_some_and(|b| matches!(b, b'\n' | b'\r')) {
                end += 1;
            }
            self.push(id("T_CLOSE_TAG"), start, end);
            self.i = end;
            self.mode = Mode::InlineHtml;
            return;
        }

        if c == b'#' && self.source.get(self.i + 1) == Some(&b'[') {
            self.push(id("T_ATTRIBUTE"), self.i, self.i + 2);
            self.i += 2;
            return;
        }

        if c == b'#' || (c == b'/' && self.source.get(self.i + 1) == Some(&b'/')) {
            let start = self.i;
            let mut j = self.i;
            while j < self.source.len() {
                if matches!(self.source[j], b'\n' | b'\r') {
                    break;
                }
                if self.source[j] == b'?' && self.source.get(j + 1) == Some(&b'>') {
                    break;
                }
                j += 1;
            }
            self.push(id("T_COMMENT"), start, j);
            self.i = j;
            self.property = was_property;
            return;
        }

        if c == b'/' && self.source.get(self.i + 1) == Some(&b'*') {
            let start = self.i;
            let end = block_comment_end(self.source, start);
            let text = &self.source[start..end];
            if !text.ends_with(b"*/") {
                self.errors.push(LexError {
                    pos: start,
                    message: format!("Unterminated comment starting line {}", self.line),
                });
            }
            let doc = text.len() > 4
                && text.starts_with(b"/**")
                && matches!(text[3], b' ' | b'\t' | b'\n' | b'\r');
            self.push(id(if doc { "T_DOC_COMMENT" } else { "T_COMMENT" }), start, end);
            self.i = end;
            self.property = was_property;
            return;
        }

        if c == b'$' && self.source.get(self.i + 1).is_some_and(|b| is_label_start(*b)) {
            let start = self.i;
            let mut j = self.i + 1;
            while j < self.source.len() && is_label(self.source[j]) {
                j += 1;
            }
            self.push(id("T_VARIABLE"), start, j);
            self.i = j;
            return;
        }

        if c.is_ascii_digit()
            || (c == b'.' && self.source.get(self.i + 1).is_some_and(u8::is_ascii_digit))
        {
            let (token_id, end) = self.scan_number();
            self.push(token_id, self.i, end);
            self.i = end;
            return;
        }

        if c == b'\'' {
            self.scan_single_quoted(self.i, self.i);
            return;
        }
        if c == b'"' {
            self.scan_double_quoted(self.i, self.i);
            return;
        }
        if matches!(c, b'b' | b'B')
            && self
                .source
                .get(self.i + 1)
                .is_some_and(|b| matches!(b, b'\'' | b'"'))
        {
            let quote = self.source[self.i + 1];
            if quote == b'\'' {
                self.scan_single_quoted(self.i, self.i + 1);
            } else {
                self.scan_double_quoted(self.i, self.i + 1);
            }
            return;
        }
        if c == b'`' {
            self.push_byte(self.i);
            self.i += 1;
            self.mode = Mode::Backtick;
            self.nowdoc = false;
            self.heredoc_at_line = false;
            return;
        }

        if matches!(c, b'b' | b'B') && self.source.get(self.i + 1..self.i + 4) == Some(b"<<<") {
            self.scan_heredoc_open_at(self.i, self.i + 1);
            return;
        }
        if c == b'<' && self.source.get(self.i + 1..self.i + 3) == Some(b"<<") {
            self.scan_heredoc_open();
            return;
        }

        if c == b'(' {
            if self.scan_cast() {
                return;
            }
            self.push_byte(self.i);
            self.i += 1;
            return;
        }

        if c == b'{' {
            self.push_byte(self.i);
            self.i += 1;
            self.depth += 1;
            return;
        }

        if c == b'}' {
            self.push_byte(self.i);
            self.i += 1;
            if self.depth > 0 {
                self.depth -= 1;
            } else if let Some(frame) = self.frames.pop() {
                self.mode = frame.mode;
                self.heredoc_label = frame.label;
                self.nowdoc = frame.nowdoc;
                self.depth = frame.depth;
                self.heredoc_at_line = false;
            }
            return;
        }

        if is_label_start(c) || c == b'\\' {
            self.scan_name(was_property);
            return;
        }

        if c == b'&'
            && !self
                .source
                .get(self.i + 1)
                .is_some_and(|b| matches!(b, b'&' | b'='))
        {
            let start = self.i;
            let at = skip_trivia(self.source, self.i + 1);
            let by_ref = self.source.get(at) == Some(&b'$')
                || self.source.get(at..at + 3) == Some(b"...");
            self.push(
                id(if by_ref {
                    "T_AMPERSAND_FOLLOWED_BY_VAR_OR_VARARG"
                } else {
                    "T_AMPERSAND_NOT_FOLLOWED_BY_VAR_OR_VARARG"
                }),
                start,
                start + 1,
            );
            self.i += 1;
            return;
        }

        self.scan_operator_tail();
    }

    /// Scans identifiers, qualified names, and PHP's contextual keyword rewrites.
    fn scan_name(&mut self, was_property: bool) {
        let start = self.i;
        let mut j = self.i;
        let fully_qualified = self.source[j] == b'\\';
        if fully_qualified {
            if !self.source.get(j + 1).is_some_and(|b| is_label_start(*b)) {
                self.push(id("T_NS_SEPARATOR"), start, start + 1);
                self.i += 1;
                return;
            }
            j += 1;
        }
        while j < self.source.len() && is_label(self.source[j]) {
            j += 1;
        }
        let mut qualified = false;
        while !was_property
            && self.source.get(j) == Some(&b'\\')
            && self.source.get(j + 1).is_some_and(|b| is_label_start(*b))
        {
            qualified = true;
            j += 1;
            while j < self.source.len() && is_label(self.source[j]) {
                j += 1;
            }
        }
        let text = &self.source[start..j];
        if fully_qualified {
            self.push(id("T_NAME_FULLY_QUALIFIED"), start, j);
            self.i = j;
            return;
        }
        if qualified {
            let head_end = text.iter().position(|b| *b == b'\\').unwrap_or(text.len());
            let relative = text[..head_end].eq_ignore_ascii_case(b"namespace");
            self.push(
                id(if relative { "T_NAME_RELATIVE" } else { "T_NAME_QUALIFIED" }),
                start,
                j,
            );
            self.i = j;
            return;
        }

        if !was_property && text.eq_ignore_ascii_case(b"yield") {
            let from = skip_trivia(self.source, j);
            if from > j
                && self.source.get(from..from + 4).is_some_and(|s| s.eq_ignore_ascii_case(b"from"))
                && !self.source.get(from + 4).is_some_and(|b| is_label(*b))
            {
                self.push(id("T_YIELD_FROM"), start, from + 4);
                self.i = from + 4;
                return;
            }
        }

        let mut token_id = if was_property {
            id("T_STRING")
        } else {
            keyword_id(text).unwrap_or_else(|| id("T_STRING"))
        };

        if token_id == id("T_ENUM") && !was_property {
            let at = skip_trivia(self.source, j);
            let rest = &self.source[at..];
            let starts_with_excluded = rest.get(..7).is_some_and(|s| s.eq_ignore_ascii_case(b"extends"))
                || rest.get(..10).is_some_and(|s| s.eq_ignore_ascii_case(b"implements"));
            if !rest.first().is_some_and(|b| is_label_start(*b)) || starts_with_excluded {
                token_id = id("T_STRING");
            }
        }

        if !was_property && matches!(token_id, t if t == id("T_PUBLIC") || t == id("T_PRIVATE") || t == id("T_PROTECTED")) {
            if let Some((set_id, end)) = self.asymmetric_set(j) {
                self.push(set_id, start, end);
                self.i = end;
                return;
            }
        }

        self.push(token_id, start, j);
        self.i = j;
        if token_id == id("T_HALT_COMPILER") {
            self.halt = Some(3);
        }
    }

    /// Detects the exact `public(set)`, `private(set)` and `protected(set)` spellings.
    fn asymmetric_set(&self, after_visibility: usize) -> Option<(i64, usize)> {
        let mut j = after_visibility;
        if self.source.get(j) != Some(&b'(') {
            return None;
        }
        j += 1;
        if !self.source.get(j..j + 3).is_some_and(|s| s.eq_ignore_ascii_case(b"set")) {
            return None;
        }
        j += 3;
        if self.source.get(j).is_some_and(|b| is_label(*b)) {
            return None;
        }
        if self.source.get(j) != Some(&b')') {
            return None;
        }
        let id = match self.source[self.i..].first() {
            _ if self.source[self.i..].starts_with_ignore_ascii_case(b"public") => id("T_PUBLIC_SET"),
            _ if self.source[self.i..].starts_with_ignore_ascii_case(b"private") => id("T_PRIVATE_SET"),
            _ => id("T_PROTECTED_SET"),
        };
        Some((id, j + 1))
    }

    /// Scans `(int)`-style casts; returns false when the parenthesis is not a cast.
    fn scan_cast(&mut self) -> bool {
        let start = self.i;
        let mut j = self.i + 1;
        while self.source.get(j).is_some_and(|b| matches!(b, b' ' | b'\t')) {
            j += 1;
        }
        let word_start = j;
        while j < self.source.len() && is_label(self.source[j]) {
            j += 1;
        }
        if j == word_start {
            return false;
        }
        let word = &self.source[word_start..j];
        while self.source.get(j).is_some_and(|b| matches!(b, b' ' | b'\t')) {
            j += 1;
        }
        if self.source.get(j) != Some(&b')') {
            return false;
        }
        let Some(cast) = cast_id(word) else {
            return false;
        };
        if word.eq_ignore_ascii_case(b"real") {
            self.errors.push(LexError {
                pos: start,
                message: "The (real) cast has been removed, use (float) instead".into(),
            });
        } else if let Some((canonical, replacement)) = non_canonical_cast(word) {
            self.warnings.push(LexWarning {
                message: format!(
                    "Non-canonical cast ({canonical}) is deprecated, use the ({replacement}) cast instead"
                ),
                pos: start,
            });
        }
        self.push(cast, start, j + 1);
        self.i = j + 1;
        true
    }

    /// Scans three-byte, two-byte, then single-byte operators.
    fn scan_operator_tail(&mut self) {
        let start = self.i;
        if start + 3 <= self.source.len() {
            if let Some(token_id) = operator_id(&self.source[start..start + 3]) {
                self.push(token_id, start, start + 3);
                self.i = start + 3;
                self.property = token_id == id("T_NULLSAFE_OBJECT_OPERATOR");
                return;
            }
        }
        if start + 2 <= self.source.len() {
            if let Some(token_id) = operator_id(&self.source[start..start + 2]) {
                self.push(token_id, start, start + 2);
                self.i = start + 2;
                self.property = token_id == id("T_OBJECT_OPERATOR");
                return;
            }
        }
        let byte = self.source[start];
        if is_bad_character(byte) {
            self.errors.push(LexError {
                pos: start,
                message: format!("syntax error, unexpected character 0x{byte:02X}"),
            });
            self.push(id("T_BAD_CHARACTER"), start, start + 1);
        } else {
            self.push_byte(start);
        }
        self.i = start + 1;
    }

    /// Scans one number and returns its token ID and exclusive end offset.
    fn scan_number(&mut self) -> (i64, usize) {
        let start = self.i;
        let source = self.source;
        if source[start] == b'.' {
            let end = scan_decimal_digits(source, start + 1);
            let end = scan_exponent(source, end);
            return (id("T_DNUMBER"), end);
        }
        if source[start] == b'0' {
            if let Some((radix, digits_start)) = numeric_prefix(source, start) {
                let end = scan_radix_digits(source, digits_start, radix);
                let value = parse_digits(&source[digits_start..end], radix);
                let token_id = if value.is_some_and(|v| v <= i64::MAX as u128) {
                    id("T_LNUMBER")
                } else {
                    id("T_DNUMBER")
                };
                return (token_id, end);
            }
        }
        let mut end = scan_decimal_digits(source, start);
        if source.get(end) == Some(&b'.') {
            end += 1;
            end = scan_decimal_digits(source, end);
            end = scan_exponent(source, end);
            return (id("T_DNUMBER"), end);
        }
        if let Some(exponent_end) = exponent_end(source, end) {
            return (id("T_DNUMBER"), exponent_end);
        }
        let digits: Vec<u8> = source[start..end].iter().copied().filter(|b| *b != b'_').collect();
        let legacy_octal = digits.len() > 1 && digits[0] == b'0';
        let invalid_octal = legacy_octal && digits.iter().any(|b| matches!(b, b'8' | b'9'));
        if legacy_octal && !invalid_octal {
            let value = parse_digits(&digits, 8);
            if value.is_none_or(|v| v > i64::MAX as u128) {
                return (id("T_DNUMBER"), end);
            }
            return (id("T_LNUMBER"), end);
        }
        if invalid_octal {
            self.errors.push(LexError { pos: start, message: "Invalid numeric literal".into() });
        }
        let value = parse_digits(&digits, 10);
        let token_id = if value.is_some_and(|v| v <= i64::MAX as u128) {
            id("T_LNUMBER")
        } else {
            id("T_DNUMBER")
        };
        (token_id, end)
    }

    /// Scans a single-quoted string, including an optional binary prefix, as one token.
    fn scan_single_quoted(&mut self, start: usize, quote: usize) {
        let mut j = quote + 1;
        let mut terminated = false;
        while j < self.source.len() {
            if self.source[j] == b'\\' && j + 1 < self.source.len() {
                j += 2;
                continue;
            }
            if self.source[j] == b'\'' {
                j += 1;
                terminated = true;
                break;
            }
            j += 1;
        }
        self.push(
            id(if terminated { "T_CONSTANT_ENCAPSED_STRING" } else { "T_ENCAPSED_AND_WHITESPACE" }),
            start,
            j,
        );
        self.i = j;
    }

    /// Scans a double-quoted string: one literal token, or a quote plus interpolation frames.
    fn scan_double_quoted(&mut self, start: usize, quote: usize) {
        let mut j = quote + 1;
        let mut interpolates = false;
        while j < self.source.len() {
            if self.source[j] == b'\\' && j + 1 < self.source.len() {
                j += 2;
                continue;
            }
            if self.source[j] == b'"' {
                break;
            }
            if self.source[j] == b'$'
                && self.source.get(j + 1).is_some_and(|b| is_label_start(*b) || *b == b'{')
            {
                interpolates = true;
                break;
            }
            if self.source[j] == b'{' && self.source.get(j + 1) == Some(&b'$') {
                interpolates = true;
                break;
            }
            j += 1;
        }
        if !interpolates {
            if j < self.source.len() {
                validate_unicode_escapes(self.source, quote + 1, j, &mut self.errors);
                self.push(id("T_CONSTANT_ENCAPSED_STRING"), start, j + 1);
                self.i = j + 1;
                return;
            }
            self.push(i64::from(self.source[quote]), start, quote + 1);
            self.i = quote + 1;
            self.mode = Mode::DoubleQuote;
            self.nowdoc = false;
            self.heredoc_at_line = false;
            return;
        }
        self.push(i64::from(self.source[quote]), start, quote + 1);
        self.i = quote + 1;
        self.mode = Mode::DoubleQuote;
        self.nowdoc = false;
        self.heredoc_at_line = false;
    }

    /// Scans `<<<LABEL`, `<<<"LABEL"` and `<<<'LABEL'` and enters the body mode.
    fn scan_heredoc_open(&mut self) {
        self.scan_heredoc_open_at(self.i, self.i);
    }

    /// Scans a heredoc opener whose `<<<` starts at `opener` and whose token starts at `start`.
    fn scan_heredoc_open_at(&mut self, start: usize, opener: usize) {
        let mut j = opener + 3;
        while self.source.get(j).is_some_and(|b| matches!(b, b' ' | b'\t')) {
            j += 1;
        }
        let quote = self.source.get(j).copied().filter(|b| matches!(b, b'\'' | b'"'));
        if quote.is_some() {
            j += 1;
        }
        let label_start = j;
        if !self.source.get(label_start).is_some_and(|b| is_label_start(*b)) {
            self.heredoc_fallback(start, opener);
            return;
        }
        while j < self.source.len() && is_label(self.source[j]) {
            j += 1;
        }
        if let Some(q) = quote {
            if self.source.get(j) != Some(&q) {
                self.heredoc_fallback(start, opener);
                return;
            }
            j += 1;
        }
        let newline = match self.source.get(j) {
            Some(b'\r') if self.source.get(j + 1) == Some(&b'\n') => 2,
            Some(b'\r' | b'\n') => 1,
            _ => {
                self.heredoc_fallback(start, opener);
                return;
            }
        };
        j += newline;
        let label_end = j - newline - usize::from(quote.is_some());
        self.heredoc_label = self.source[label_start..label_end].to_vec();
        self.nowdoc = quote == Some(b'\'');
        self.push(id("T_START_HEREDOC"), start, j);
        self.i = j;
        self.mode = Mode::Heredoc;
        self.heredoc_at_line = true;
    }

    /// Falls back to the normal `<<` operator when a `<<<` cannot open a heredoc.
    fn heredoc_fallback(&mut self, start: usize, opener: usize) {
        self.errors.push(LexError {
            pos: start,
            message: "syntax error, unexpected token \"<<\"".into(),
        });
        if start < opener {
            self.push(id("T_STRING"), start, opener);
            self.i = opener;
        } else {
            self.push(id("T_SL"), start, start + 2);
            self.i = start + 2;
        }
    }

    /// Scans one string body run, then handles its terminator or interpolation marker.
    fn scan_string_body(&mut self) {
        let start = self.i;
        let mut stop = None;
        let mut kind = 0u8;
        while self.i < self.source.len() {
            if self.mode == Mode::Heredoc && self.heredoc_at_line {
                if heredoc_end(self.source, self.i, &self.heredoc_label).is_some() {
                    stop = Some(self.i);
                    kind = 1;
                    break;
                }
            }
            self.heredoc_at_line = false;
            let c = self.source[self.i];
            if c == b'\\' && self.i + 1 < self.source.len() && !self.nowdoc {
                let next = self.source[self.i + 1];
                if self.mode != Mode::Heredoc || !matches!(next, b'\n' | b'\r') {
                    self.i += 2;
                    continue;
                }
            }
            if self.mode == Mode::DoubleQuote && c == b'"' {
                stop = Some(self.i);
                kind = 1;
                break;
            }
            if self.mode == Mode::Backtick && c == b'`' {
                stop = Some(self.i);
                kind = 1;
                break;
            }
            if !self.nowdoc {
                if c == b'$' && self.source.get(self.i + 1).is_some_and(|b| is_label_start(*b)) {
                    stop = Some(self.i);
                    kind = 2;
                    break;
                }
                if c == b'$' && self.source.get(self.i + 1) == Some(&b'{') {
                    stop = Some(self.i);
                    kind = 3;
                    break;
                }
                if c == b'{' && self.source.get(self.i + 1) == Some(&b'$') {
                    stop = Some(self.i);
                    kind = 4;
                    break;
                }
            }
            if matches!(c, b'\n' | b'\r') {
                self.heredoc_at_line = true;
            }
            self.i += 1;
        }
        let stop = stop.unwrap_or(self.source.len());
        if stop > start {
            if !self.nowdoc {
                validate_unicode_escapes(self.source, start, stop, &mut self.errors);
            }
            self.push(id("T_ENCAPSED_AND_WHITESPACE"), start, stop);
        }
        self.i = stop;
        if self.i >= self.source.len() {
            return;
        }
        match kind {
            1 => self.finish_string(),
            2 => self.scan_simple_interpolation(),
            3 => self.scan_dollar_brace(),
            4 => {
                self.push(id("T_CURLY_OPEN"), self.i, self.i + 1);
                self.push_interpolation_frame(1);
            }
            _ => {}
        }
    }

    /// Emits the string terminator and returns to PHP mode.
    fn finish_string(&mut self) {
        if self.mode == Mode::Heredoc {
            let length = heredoc_end(self.source, self.i, &self.heredoc_label).unwrap_or(0);
            self.push(id("T_END_HEREDOC"), self.i, self.i + length);
            self.i += length;
        } else {
            self.push_byte(self.i);
            self.i += 1;
        }
        self.mode = Mode::Php;
        self.heredoc_at_line = false;
    }

    /// Emits `$name`, one `[...]` offset or one member access, matching PHP's short interpolation.
    fn scan_simple_interpolation(&mut self) {
        let start = self.i;
        let mut j = start + 1;
        while j < self.source.len() && is_label(self.source[j]) {
            j += 1;
        }
        self.push(id("T_VARIABLE"), start, j);
        self.i = j;
        if self.source.get(self.i) == Some(&b'[') {
            self.push_byte(self.i);
            self.i += 1;
            self.scan_string_offset();
            return;
        }
        if self.source.get(self.i) == Some(&b'?')
            && self.source.get(self.i + 1) == Some(&b'-')
            && self.source.get(self.i + 2) == Some(&b'>')
            && self.source.get(self.i + 3).is_some_and(|b| is_label_start(*b))
        {
            self.push(id("T_NULLSAFE_OBJECT_OPERATOR"), self.i, self.i + 3);
            self.i += 3;
            self.scan_interpolation_property();
            return;
        }
        if self.source.get(self.i) == Some(&b'-')
            && self.source.get(self.i + 1) == Some(&b'>')
            && self.source.get(self.i + 2).is_some_and(|b| is_label_start(*b))
        {
            self.push(id("T_OBJECT_OPERATOR"), self.i, self.i + 2);
            self.i += 2;
            self.scan_interpolation_property();
        }
    }

    /// Emits one label after an interpolated member access.
    fn scan_interpolation_property(&mut self) {
        let name = self.i;
        let mut k = self.i;
        while k < self.source.len() && is_label(self.source[k]) {
            k += 1;
        }
        self.push(id("T_STRING"), name, k);
        self.i = k;
    }

    /// Scans Zend's `ST_VAR_OFFSET` vocabulary until the closing bracket.
    fn scan_string_offset(&mut self) {
        let mut first = true;
        while self.i < self.source.len() {
            let c = self.source[self.i];
            if c == b']' {
                if first {
                    self.errors.push(LexError {
                        pos: self.i,
                        message: "syntax error, unexpected token \"]\", expecting \"-\" or identifier or variable or number".into(),
                    });
                }
                self.push_byte(self.i);
                self.i += 1;
                return;
            }
            if c == b'$' && self.source.get(self.i + 1).is_some_and(|b| is_label_start(*b)) {
                let variable = self.i;
                let mut k = self.i + 1;
                while k < self.source.len() && is_label(self.source[k]) {
                    k += 1;
                }
                self.push(id("T_VARIABLE"), variable, k);
                self.i = k;
                first = false;
                continue;
            }
            if matches!(c, b'-' | b'+')
                && self.source.get(self.i + 1).is_some_and(u8::is_ascii_digit)
            {
                self.push_byte(self.i);
                self.i += 1;
                first = false;
                continue;
            }
            if c.is_ascii_digit() {
                let number = self.i;
                let mut k = self.i + 1;
                while k < self.source.len() && is_label(self.source[k]) {
                    k += 1;
                }
                self.push(id("T_NUM_STRING"), number, k);
                self.i = k;
                first = false;
                continue;
            }
            if is_label_start(c) {
                let name = self.i;
                let mut k = self.i + 1;
                while k < self.source.len() && is_label(self.source[k]) {
                    k += 1;
                }
                self.push(id("T_STRING"), name, k);
                self.i = k;
                first = false;
                continue;
            }
            if matches!(c, b'\'' | b' ' | b'\t' | b'\r' | b'\n') {
                self.errors.push(LexError {
                    pos: self.i,
                    message: "syntax error, unexpected string content \"\", expecting \"-\" or identifier or variable or number".into(),
                });
                self.push(id("T_ENCAPSED_AND_WHITESPACE"), self.i, self.i);
                return;
            }
            if c == b'"' {
                self.errors.push(LexError {
                    pos: self.i,
                    message: "syntax error, unexpected double-quote mark, expecting \"-\" or identifier or variable or number".into(),
                });
                self.push_byte(self.i);
                self.i += 1;
                first = false;
                continue;
            }
            self.push_byte(self.i);
            self.i += 1;
            first = false;
        }
    }

    /// Emits `${name}` as three tokens, or opens an expression interpolation frame.
    fn scan_dollar_brace(&mut self) {
        let start = self.i;
        let name_start = start + 2;
        let mut j = name_start;
        while j < self.source.len() && is_label(self.source[j]) {
            j += 1;
        }
        if j > name_start && matches!(self.source.get(j), Some(b'[' | b'}')) {
            self.push(id("T_DOLLAR_OPEN_CURLY_BRACES"), start, start + 2);
            self.push(id("T_STRING_VARNAME"), name_start, j);
            self.i = j;
            if self.source.get(j) == Some(&b'}') {
                self.push_byte(j);
                self.i = j + 1;
            } else {
                self.push_interpolation_frame(0);
            }
            return;
        }
        self.push(id("T_DOLLAR_OPEN_CURLY_BRACES"), start, start + 2);
        self.push_interpolation_frame(2);
    }

    /// Suspends the current string, switches to PHP mode for an interpolation expression.
    fn push_interpolation_frame(&mut self, consumed: usize) {
        self.frames.push(Frame {
            mode: self.mode,
            label: std::mem::take(&mut self.heredoc_label),
            nowdoc: self.nowdoc,
            depth: self.depth,
        });
        self.mode = Mode::Php;
        self.depth = 0;
        self.i += consumed;
        self.heredoc_at_line = false;
    }
}

/// Returns the canonical spelling and replacement for a deprecated non-canonical cast.
fn non_canonical_cast(word: &[u8]) -> Option<(&'static str, &'static str)> {
    if word.eq_ignore_ascii_case(b"integer") {
        Some(("integer", "int"))
    } else if word.eq_ignore_ascii_case(b"boolean") {
        Some(("boolean", "bool"))
    } else if word.eq_ignore_ascii_case(b"double") {
        Some(("double", "float"))
    } else if word.eq_ignore_ascii_case(b"binary") {
        Some(("binary", "string"))
    } else {
        None
    }
}

/// Validates PHP's `\u{...}` escapes in one literal run.
fn validate_unicode_escapes(source: &[u8], start: usize, end: usize, errors: &mut Vec<LexError>) {
    let mut i = start;
    while i < end {
        if source[i] != b'\\' {
            i += 1;
            continue;
        }
        if i + 1 >= end {
            break;
        }
        if source[i + 1] != b'u' || source.get(i + 2) != Some(&b'{') {
            i += 2;
            continue;
        }
        let digits = i + 3;
        let mut j = digits;
        let mut value = 0u64;
        let mut overflow = false;
        while j < end {
            let Some(digit) = (source[j] as char).to_digit(16) else {
                break;
            };
            value = match value.checked_mul(16).and_then(|value| value.checked_add(u64::from(digit))) {
                Some(value) => value,
                None => {
                    overflow = true;
                    value
                }
            };
            j += 1;
        }
        if j == digits || source.get(j) != Some(&b'}') {
            errors.push(LexError {
                pos: i,
                message: "Invalid UTF-8 codepoint escape sequence".into(),
            });
            i = j.max(i + 1);
            continue;
        }
        if overflow || value > 0x10ffff {
            errors.push(LexError {
                pos: i,
                message: "Invalid UTF-8 codepoint escape sequence: Codepoint too large".into(),
            });
        }
        i = j + 1;
    }
}

/// Tokenizes one source buffer and returns lexical diagnostics for TOKEN_PARSE.
pub(crate) fn scan(source: &[u8]) -> (Vec<Token>, Vec<LexError>, Vec<LexWarning>) {
    Lexer::new(source).run()
}

/// Formats a PHP parser diagnostic for one unexpected token and an expected literal.
fn unexpected_expecting(source: &[u8], token: &Token, expected: &str) -> String {
    let text = &source[token.pos..token.pos + token.len];
    if token.id == id("T_STRING") {
        format!("syntax error, unexpected identifier \"{}\", expecting {expected}", String::from_utf8_lossy(text))
    } else if text.len() <= 3 {
        format!("syntax error, unexpected token \"{}\", expecting {expected}", String::from_utf8_lossy(text))
    } else {
        format!(
            "syntax error, unexpected token \"{}\", expecting {expected}",
            crate::token_name(token.id).unwrap_or("UNKNOWN")
        )
    }
}

/// Returns whether a token belongs to PHP's trivia set for halt-compiler accounting.
fn is_trivia(token_id: i64) -> bool {
    [id("T_WHITESPACE"), id("T_COMMENT"), id("T_DOC_COMMENT"), id("T_OPEN_TAG")].contains(&token_id)
}

/// Returns whether a byte is a bad character in PHP code.
fn is_bad_character(byte: u8) -> bool {
    (byte < 0x20 && !matches!(byte, b'\t' | b'\n' | b'\r')) || byte == 0x7f
}

/// Finds one byte sequence inside a larger buffer.
fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|window| window == needle)
}

/// Returns the end offset of a `/* ... */` comment, including the closer when present.
fn block_comment_end(source: &[u8], start: usize) -> usize {
    let mut j = start + 2;
    while j + 1 < source.len() {
        if source[j] == b'*' && source[j + 1] == b'/' {
            return j + 2;
        }
        j += 1;
    }
    source.len()
}

/// Skips whitespace and comments for PHP's contextual lookahead rules.
fn skip_trivia(source: &[u8], mut at: usize) -> usize {
    loop {
        let before = at;
        while source.get(at).is_some_and(|b| is_whitespace(*b)) {
            at += 1;
        }
        if source.get(at..at + 2) == Some(b"/*") {
            at = block_comment_end(source, at);
        } else if source.get(at..at + 2) == Some(b"//")
            || source.get(at) == Some(&b'#') && source.get(at + 1) != Some(&b'[')
        {
            while at < source.len()
                && !matches!(source[at], b'\n' | b'\r')
                && source.get(at..at + 2) != Some(b"?>")
            {
                at += 1;
            }
        }
        if at == before {
            return at;
        }
    }
}

/// Returns the prefix radix and first digit offset for `0x`, `0b` and `0o`.
fn numeric_prefix(source: &[u8], start: usize) -> Option<(u32, usize)> {
    let marker = *source.get(start + 1)?;
    let digit = *source.get(start + 2)?;
    match marker {
        b'x' | b'X' if digit.is_ascii_hexdigit() => Some((16, start + 2)),
        b'b' | b'B' if matches!(digit, b'0' | b'1') => Some((2, start + 2)),
        b'o' | b'O' if matches!(digit, b'0'..=b'7') => Some((8, start + 2)),
        _ => None,
    }
}

/// Scans decimal digits allowing underscores only between digit runs.
fn scan_decimal_digits(source: &[u8], mut j: usize) -> usize {
    if !source.get(j).is_some_and(u8::is_ascii_digit) {
        return j;
    }
    j += 1;
    loop {
        if source.get(j).is_some_and(u8::is_ascii_digit) {
            j += 1;
        } else if source.get(j) == Some(&b'_')
            && source.get(j + 1).is_some_and(u8::is_ascii_digit)
        {
            j += 2;
        } else {
            return j;
        }
    }
}

/// Scans radix-specific digits allowing underscores only between digit runs.
fn scan_radix_digits(source: &[u8], mut j: usize, radix: u32) -> usize {
    let valid = |b: &u8| match radix {
        2 => matches!(b, b'0' | b'1'),
        8 => matches!(b, b'0'..=b'7'),
        16 => b.is_ascii_hexdigit(),
        _ => b.is_ascii_digit(),
    };
    if !source.get(j).is_some_and(valid) {
        return j;
    }
    j += 1;
    loop {
        if source.get(j).is_some_and(valid) {
            j += 1;
        } else if source.get(j) == Some(&b'_') && source.get(j + 1).is_some_and(valid) {
            j += 2;
        } else {
            return j;
        }
    }
}

/// Parses underscore-free digit text in the given radix.
fn parse_digits(digits: &[u8], radix: u32) -> Option<u128> {
    let mut value = 0u128;
    for byte in digits.iter().copied().filter(|b| *b != b'_') {
        let digit = char::from(byte).to_digit(radix)?;
        value = value.checked_mul(u128::from(radix))?.checked_add(u128::from(digit))?;
    }
    Some(value)
}

/// Returns the exclusive end of an exponent when one is present and valid.
fn exponent_end(source: &[u8], at: usize) -> Option<usize> {
    if !source.get(at).is_some_and(|b| matches!(b, b'e' | b'E')) {
        return None;
    }
    let mut j = at + 1;
    if source.get(j).is_some_and(|b| matches!(b, b'+' | b'-')) {
        j += 1;
    }
    let end = scan_decimal_digits(source, j);
    (end > j).then_some(end)
}

/// Scans an exponent when present.
fn scan_exponent(source: &[u8], at: usize) -> usize {
    exponent_end(source, at).unwrap_or(at)
}

/// Returns the length of a heredoc closing label (including indentation) at `pos`.
fn heredoc_end(source: &[u8], pos: usize, label: &[u8]) -> Option<usize> {
    let mut j = pos;
    while source.get(j).is_some_and(|b| matches!(b, b' ' | b'\t')) {
        j += 1;
    }
    if source.get(j..j + label.len()) != Some(label) {
        return None;
    }
    let after = j + label.len();
    if after >= source.len() || source.get(after).is_some_and(|b| is_label(*b)) {
        return None;
    }
    Some(after - pos)
}

/// Case-insensitive prefix helper used by asymmetric-visibility detection.
trait StartsWithIgnoreAsciiCase {
    /// Returns whether this byte slice starts with the ASCII text, ignoring case.
    fn starts_with_ignore_ascii_case(&self, prefix: &[u8]) -> bool;
}

impl StartsWithIgnoreAsciiCase for [u8] {
    /// Compares a leading byte range against an ASCII prefix without allocating.
    fn starts_with_ignore_ascii_case(&self, prefix: &[u8]) -> bool {
        self.get(..prefix.len()).is_some_and(|head| head.eq_ignore_ascii_case(prefix))
    }
}
