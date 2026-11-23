//! Purpose:
//! Runs the byte scanner for compiler input, runtime fragments, and PHP tokens.
//!
//! Called from:
//! - `super::scan`, `super::scan_runtime`, and the compiler syntax projection.
//!
//! Key details:
//! - The PHP tokenizer preserves every source byte and Zend line numbers.
//! - The compiler receives lexemes directly and applies its source-span and trivia rules.
//! - Inline HTML, open/close tags, heredoc/nowdoc bodies and interpolation retain Zend boundaries.
//! - Malformed input remains tokenizable without TOKEN_PARSE; lexical errors are reported separately.

use super::keywords::{
    cast_id, is_label, is_label_start, is_whitespace, keyword_id,
};
use super::lexeme::{compiler_trivia_len, php_trivia, runtime_trivia, Trivia};
use super::numbers::{numeric_prefix, scan_number, Number, NumberKind, NumberProfile};
use super::strings::{
    braced_interpolation, heredoc_closing, heredoc_opening, quoted_len, BracedInterpolation,
    HeredocProfile,
};
use super::{digits_len, id, identifier_len, newline_count, operator, Operator, Token};

/// One lexical diagnostic that TOKEN_PARSE must raise as a ParseError.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LexError {
    /// Byte offset of the offending token or character.
    pub pos: usize,
    /// PHP's exact parser-facing message.
    pub message: String,
}

/// One non-fatal TOKEN_PARSE diagnostic raised while scanning.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LexWarning {
    /// PHP's exact parser-facing message.
    pub message: String,
    /// Byte offset of the construct that raised it.
    pub pos: usize,
    /// PHP error level Zend raises for this diagnostic.
    pub severity: i64,
    /// Whether the diagnostic only exists under `TOKEN_PARSE` (non-canonical casts).
    pub parse_only: bool,
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

/// A boundary recognized by the scanner for the compiler's syntax token projection.
pub(crate) enum CompilerLexeme {
    OpenTag,
    MissingOpenTag,
    ForbiddenTag,
    Attribute,
    Variable,
    Word,
    Number(NumberKind),
    SingleQuoted { terminated: bool },
    DoubleQuoted,
    Heredoc,
    Operator(Operator),
    Byte(u8),
}

/// Receives lexemes on the compiler path without allocating a second token stream.
pub(crate) trait CompilerOutput {
    /// Selects the compiler path only for its projection type.
    const ACTIVE: bool = true;

    /// Emits syntax tokens and returns the consumed end, or stops scanning on an error.
    ///
    /// Double-quote and heredoc events give the decoder the opening delimiter.
    /// The decoder returns the physical end after consuming the rest of the literal.
    fn emit(&mut self, lexeme: CompilerLexeme, start: usize, end: usize) -> Option<usize>;
}

impl CompilerOutput for () {
    const ACTIVE: bool = false;

    /// The native PHP and runtime builds collect the original PHP tokens directly.
    fn emit(&mut self, _: CompilerLexeme, _: usize, _: usize) -> Option<usize> {
        unreachable!("compiler projection is inactive")
    }
}

/// The scanner state over one immutable source buffer.
struct Lexer<'a, C> {
    source: &'a [u8],
    i: usize,
    line: usize,
    mode: Mode,
    runtime: bool,
    forbid_tags: bool,
    number_profile: NumberProfile,
    compiler: C,
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

impl<'a, C: CompilerOutput> Lexer<'a, C> {
    /// Creates a scanner for one source buffer.
    fn new(source: &'a [u8], compiler: C) -> Self {
        Self {
            source,
            i: 0,
            line: 1,
            mode: Mode::InlineHtml,
            runtime: false,
            forbid_tags: false,
            number_profile: NumberProfile::Zend,
            compiler,
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

    /// Runs the shared scanner until it reaches EOF or the compiler reports an error.
    fn run(&mut self) {
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
        if !C::ACTIVE && !self.runtime {
            self.validate_heredocs();
            self.validate_halt_compiler();
        }
    }

    /// Validates `__halt_compiler ( ) ;` for TOKEN_PARSE.
    ///
    /// A `T_HALT_COMPILER` after `->`, `?->` or `\` is an ordinary member/name, not the
    /// statement, so only the first statement-position occurrence is validated.
    fn validate_halt_compiler(&mut self) {
        let Some(halt_index) = self.halt_statement_index() else {
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
                pos: diagnostic_pos(self.source, open),
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
                pos: diagnostic_pos(self.source, close),
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
        // PHP also terminates the compiler halt at a closing tag: `__halt_compiler() ?>`.
        if text(semicolon) != b";" && semicolon.id != id("T_CLOSE_TAG") {
            self.errors.push(LexError {
                pos: semicolon.pos,
                message: unexpected_expecting(self.source, semicolon, "\";\""),
            });
        }
    }

    /// Returns the index of the first `__halt_compiler` that is not a member/name segment.
    fn halt_statement_index(&self) -> Option<usize> {
        self.tokens.iter().enumerate().find_map(|(index, token)| {
            (token.id == id("T_HALT_COMPILER") && !self.halt_is_member_name(index))
                .then_some(index)
        })
    }

    /// Returns whether the `T_HALT_COMPILER` at `index` follows a member/name separator.
    fn halt_is_member_name(&self, index: usize) -> bool {
        self.tokens[..index]
            .iter()
            .rev()
            .find(|token| !is_trivia(token.id))
            .is_some_and(|previous| is_halt_name_separator(previous.id))
    }

    /// Validates flexible heredoc indentation and unterminated bodies for TOKEN_PARSE.
    fn validate_heredocs(&mut self) {
        let mut stack = Vec::new();
        let mut heredoc_end = vec![None; self.tokens.len()];
        for index in 0..self.tokens.len() {
            if self.tokens[index].id == id("T_START_HEREDOC") {
                stack.push(index);
            } else if self.tokens[index].id == id("T_END_HEREDOC") {
                if let Some(start) = stack.pop() {
                    heredoc_end[start] = Some(index);
                    self.validate_heredoc_pair(start, index, &heredoc_end);
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
    fn validate_heredoc_pair(&mut self, start_index: usize, end_index: usize, heredoc_end: &[Option<usize>]) {
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
        // A newline inside `{$...}` / `${...}` does not start a heredoc body line for Zend's
        // indentation check, so remember those byte ranges and skip their line starts.
        let mut expression_ranges: Vec<(usize, usize)> = Vec::new();
        let mut depth = 0usize;
        let mut open = 0usize;
        let mut cursor = start_index + 1;
        while cursor < end_index {
            let token = &self.tokens[cursor];
            // A heredoc nested inside an interpolation is validated by its own pair. Jump over
            // its whole token range so deeply nested heredocs stay linear instead of rescanning
            // every inner body from every outer pair.
            if token.id == id("T_START_HEREDOC") {
                if let Some(nested_end) = heredoc_end[cursor] {
                    cursor = nested_end + 1;
                    continue;
                }
            }
            if token.id == id("T_CURLY_OPEN") || token.id == id("T_DOLLAR_OPEN_CURLY_BRACES") {
                if depth == 0 {
                    open = token.pos;
                }
                depth += 1;
            } else if depth > 0 && token.id == i64::from(b'{') {
                depth += 1;
            } else if depth > 0 && token.id == i64::from(b'}') {
                depth -= 1;
                if depth == 0 {
                    expression_ranges.push((open, token.pos + token.len));
                }
            }
            cursor += 1;
        }
        let body = &self.source[body_start..end.pos];
        let mut index = 0;
        // Ranges are ordered, so one moving cursor replaces a per-line scan over all of them.
        let mut range_index = 0usize;
        while index < body.len() {
            let line_start = body_start + index;
            while range_index < expression_ranges.len()
                && line_start >= expression_ranges[range_index].1
            {
                range_index += 1;
            }
            if let Some(&(start, end)) = expression_ranges.get(range_index) {
                if line_start >= start && line_start < end {
                    // Skip every line inside the interpolation in one step. Nested heredoc
                    // bodies in the range were validated by their own pairs.
                    let end_offset = end.saturating_sub(body_start).min(body.len());
                    index = Self::next_body_line(body, end_offset);
                    range_index += 1;
                    continue;
                }
            }
            let mut line_end = index;
            while line_end < body.len() && !matches!(body[line_end], b'\n' | b'\r') {
                line_end += 1;
            }
            // Zend inspects only the first `indent.len()` bytes of every body line. A line
            // shorter than the indentation (a newline before the end) is left alone; any
            // other byte that differs from the closing label's whitespace is either the
            // mixed-whitespace error or a too-shallow indentation.
            for offset in 0..indent.len() {
                let Some(&byte) = body.get(index + offset) else { break };
                if byte == indent[0] {
                    continue;
                }
                if matches!(byte, b'\n' | b'\r') {
                    break;
                }
                if matches!(byte, b' ' | b'\t') {
                    self.errors.push(LexError {
                        pos: line_start,
                        message: "Invalid indentation - tabs and spaces cannot be mixed".into(),
                    });
                    return;
                }
                self.errors.push(LexError {
                    pos: line_start,
                    message: format!(
                        "Invalid body indentation level (expecting an indentation level of at least {})",
                        indent.len()
                    ),
                });
                return;
            }
            index = Self::next_body_line(body, line_end);
        }
    }

    /// Returns the offset of the body line after the one starting at `line_start`.
    fn next_body_line(body: &[u8], line_start: usize) -> usize {
    let mut line_end = line_start;
    while line_end < body.len() && !matches!(body[line_end], b'\n' | b'\r') {
        line_end += 1;
    }
    if line_end < body.len() {
        if body[line_end] == b'\r' && body.get(line_end + 1) == Some(&b'\n') {
            line_end + 2
        } else {
            line_end + 1
        }
    } else {
        body.len()
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
        let text = &self.source[start..end];
        self.line += if self.runtime { text.iter().filter(|&&byte| byte == b'\n').count() } else { newline_count(text) };
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

    /// Advances by the bytes consumed by the compiler projection.
    fn emit_compiler(&mut self, lexeme: CompilerLexeme, end: usize) {
        self.i = self.compiler.emit(lexeme, self.i, end).unwrap_or(self.source.len());
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

    /// Emits trivia for the public tokenizer or runtime without clearing member-name context.
    fn scan_php_trivia(&mut self) -> bool {
        let trivia_at = if self.runtime { runtime_trivia } else { php_trivia };
        let Some((trivia, len)) = trivia_at(&self.source[self.i..]) else {
            return false;
        };
        let start = self.i;
        let end = start + len;
        let name = match trivia {
            Trivia::Whitespace => "T_WHITESPACE",
            Trivia::LineComment => "T_COMMENT",
            Trivia::BlockComment => {
                let text = &self.source[start..end];
                if !text.ends_with(b"*/") {
                    self.errors.push(LexError {
                        pos: start,
                        message: format!("Unterminated comment starting line {}", self.line),
                    });
                }
                if text.starts_with(b"/**") && text.get(3).is_some_and(|&byte| is_whitespace(byte)) {
                    "T_DOC_COMMENT"
                } else {
                    "T_COMMENT"
                }
            }
        };
        self.push(id(name), start, end);
        self.i = end;
        true
    }

    /// Scans one PHP-mode token (or starts a string body).
    fn scan_php(&mut self) {
        if C::ACTIVE {
            let tail = &self.source[self.i..];
            let trivia = compiler_trivia_len(tail);
            if trivia != 0 {
                self.i += trivia;
                return;
            }
            if self.forbid_tags && (tail.starts_with(b"<?php") || tail.starts_with(b"?>")) {
                self.emit_compiler(CompilerLexeme::ForbiddenTag, self.i);
                return;
            }
        } else if self.scan_php_trivia() {
            return;
        }
        let c = self.source[self.i];
        let was_property = std::mem::take(&mut self.property);

        if !C::ACTIVE && c == b'?' && self.source.get(self.i + 1) == Some(&b'>') {
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
            if C::ACTIVE {
                self.emit_compiler(CompilerLexeme::Attribute, self.i + 2);
            } else {
                self.push(id("T_ATTRIBUTE"), self.i, self.i + 2);
                self.i += 2;
            }
            return;
        }

        if c == b'$' && (C::ACTIVE || self.source.get(self.i + 1).is_some_and(|&byte| {
            self.name_start(byte) || (self.runtime && byte.is_ascii_digit())
        })) {
            let start = self.i;
            let end = start + 1 + self.name_len(start + 1);
            if C::ACTIVE {
                self.emit_compiler(CompilerLexeme::Variable, end);
            } else {
                self.push(id("T_VARIABLE"), start, end);
                self.i = end;
            }
            return;
        }

        if c.is_ascii_digit()
            || (!self.runtime && c == b'.' && self.source.get(self.i + 1).is_some_and(u8::is_ascii_digit))
        {
            let number = scan_number(&self.source[self.i..], self.number_profile);
            let end = self.i + number.len;
            if C::ACTIVE {
                self.emit_compiler(CompilerLexeme::Number(number.kind), end);
            } else {
                let token_id = self.number_token_id(number);
                self.push(token_id, self.i, end);
                self.i = end;
            }
            return;
        }

        if c == b'\'' {
            self.scan_single_quoted(self.i, self.i);
            return;
        }
        if c == b'"' {
            if C::ACTIVE {
                self.emit_compiler(CompilerLexeme::DoubleQuoted, self.i + 1);
            } else {
                self.scan_double_quoted(self.i, self.i);
            }
            return;
        }
        if !C::ACTIVE && !self.runtime && matches!(c, b'b' | b'B')
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
        if !C::ACTIVE && !self.runtime && c == b'`' {
            self.push_byte(self.i);
            self.i += 1;
            self.mode = Mode::Backtick;
            self.nowdoc = false;
            self.heredoc_at_line = false;
            return;
        }

        if !C::ACTIVE && !self.runtime && matches!(c, b'b' | b'B') && self.source.get(self.i + 1..self.i + 4) == Some(b"<<<") {
            self.scan_heredoc_open_at(self.i, self.i + 1);
            return;
        }
        if !self.runtime && c == b'<' && self.source.get(self.i + 1..self.i + 3) == Some(b"<<") {
            if C::ACTIVE {
                self.emit_compiler(CompilerLexeme::Heredoc, self.i + 3);
            } else {
                self.scan_heredoc_open();
            }
            return;
        }

        if !C::ACTIVE && c == b'(' {
            if self.scan_cast() {
                return;
            }
            self.push_byte(self.i);
            self.i += 1;
            return;
        }

        if !C::ACTIVE && c == b'{' {
            self.push_byte(self.i);
            self.i += 1;
            self.depth += 1;
            return;
        }

        if !C::ACTIVE && c == b'}' {
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

        if self.name_start(c) || (!C::ACTIVE && c == b'\\') {
            self.scan_name(was_property);
            return;
        }

        if !C::ACTIVE && c == b'&'
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

    /// Identifies the first byte of a name in the active source profile.
    fn name_start(&self, byte: u8) -> bool {
        is_label_start(byte) && (!self.runtime || byte.is_ascii())
    }

    /// Returns the length of a name segment in the active source profile.
    fn name_len(&self, start: usize) -> usize {
        let source = &self.source[start..];
        if self.runtime {
            source.iter().take_while(|&&byte| byte == b'_' || byte.is_ascii_alphanumeric()).count()
        } else {
            identifier_len(source)
        }
    }

    /// Scans identifiers, qualified names, and PHP's contextual keyword rewrites.
    fn scan_name(&mut self, was_property: bool) {
        let start = self.i;
        if C::ACTIVE {
            // The syntax parser retains individual namespace segments and their spans.
            self.emit_compiler(CompilerLexeme::Word, start + self.name_len(start));
            return;
        }
        let mut j = self.i;
        let fully_qualified = self.source[j] == b'\\';
        if fully_qualified {
            if !self.source.get(j + 1).is_some_and(|&byte| self.name_start(byte)) {
                self.push(id("T_NS_SEPARATOR"), start, start + 1);
                self.i += 1;
                return;
            }
            j += 1;
        }
        j += self.name_len(j);
        let mut qualified = false;
        while !was_property
            && self.source.get(j) == Some(&b'\\')
            && self.source.get(j + 1).is_some_and(|&byte| self.name_start(byte))
        {
            qualified = true;
            j += 1;
            j += self.name_len(j);
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

        if !self.runtime && !was_property && text.eq_ignore_ascii_case(b"yield") {
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

        let follows_name_separator = token_id == id("T_HALT_COMPILER")
            && self
                .tokens
                .iter()
                .rev()
                .find(|token| !is_trivia(token.id))
                .is_some_and(|previous| is_halt_name_separator(previous.id));
        self.push(token_id, start, j);
        self.i = j;
        if !self.runtime && token_id == id("T_HALT_COMPILER") && !follows_name_separator {
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
        j += identifier_len(&self.source[j..]);
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
                // Zend emits these casts through zend_error(E_DEPRECATED, ...).
                severity: 8_192,
                parse_only: true,
            });
        }
        self.push(cast, start, j + 1);
        self.i = j + 1;
        true
    }

    /// Scans three-byte, two-byte, then single-byte operators.
    fn scan_operator_tail(&mut self) {
        let start = self.i;
        if let Some((op, len)) = operator(&self.source[start..]) {
            if C::ACTIVE {
                self.emit_compiler(CompilerLexeme::Operator(op), start + len);
            } else {
                self.push(op.php_id(), start, start + len);
                self.i += len;
                self.property = matches!(op, Operator::Arrow | Operator::QuestionArrow);
            }
            return;
        }
        let byte = self.source[start];
        if C::ACTIVE {
            self.emit_compiler(CompilerLexeme::Byte(byte), start + 1);
            return;
        }
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

    /// Classifies one scanned number using PHP's integer size and octal rules.
    fn number_token_id(&mut self, number: Number) -> i64 {
        let digits = &self.source[self.i..self.i + number.len];
        let value = match number.kind {
            NumberKind::Float => return id("T_DNUMBER"),
            NumberKind::Prefixed(radix) => parse_digits(&digits[2..], radix),
            NumberKind::LegacyOctal if digits.iter().any(|b| matches!(b, b'8' | b'9')) => {
                self.errors.push(LexError { pos: self.i, message: "Invalid numeric literal".into() });
                parse_digits(digits, 10)
            }
            NumberKind::LegacyOctal => parse_digits(digits, 8),
            NumberKind::Decimal => parse_digits(digits, 10),
        };
        if value.is_some_and(|v| v <= i64::MAX as u128) {
            id("T_LNUMBER")
        } else {
            id("T_DNUMBER")
        }
    }

    /// Scans a single-quoted string, including an optional binary prefix, as one token.
    fn scan_single_quoted(&mut self, start: usize, quote: usize) {
        let len = quoted_len(&self.source[quote + 1..], b'\'');
        let j = len.map_or(self.source.len(), |len| quote + 1 + len);
        if C::ACTIVE {
            self.emit_compiler(CompilerLexeme::SingleQuoted { terminated: len.is_some() }, j);
            return;
        }
        self.push(
            id(if len.is_some() { "T_CONSTANT_ENCAPSED_STRING" } else { "T_ENCAPSED_AND_WHITESPACE" }),
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
            if braced_interpolation(&self.source[j..]).is_some()
                || self.source[j] == b'$' && self.source.get(j + 1).is_some_and(|b| is_label_start(*b))
            {
                interpolates = true;
                break;
            }
            j += 1;
        }
        if !interpolates {
            if j < self.source.len() {
                validate_unicode_escapes(self.source, quote + 1, j, &mut self.errors);
                validate_octal_escapes(self.source, quote + 1, j, &mut self.warnings);
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
        let label_source = opener + 3;
        let Ok(opening) = heredoc_opening(&self.source[label_source..], HeredocProfile::Zend) else {
            self.heredoc_fallback(start, opener);
            return;
        };
        let j = label_source + opening.len;
        self.heredoc_label = opening.label.to_vec();
        self.nowdoc = opening.nowdoc;
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
                if heredoc_closing(&self.source[self.i..], &self.heredoc_label, HeredocProfile::Zend).is_some() {
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
                if let Some(marker) = braced_interpolation(&self.source[self.i..]) {
                    stop = Some(self.i);
                    kind = match marker {
                        BracedInterpolation::DollarCurly => 3,
                        BracedInterpolation::CurlyDollar => 4,
                    };
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
                validate_octal_escapes(self.source, start, stop, &mut self.warnings);
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
            let length = heredoc_closing(&self.source[self.i..], &self.heredoc_label, HeredocProfile::Zend).map_or(0, |(len, _)| len);
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
        j += identifier_len(&self.source[j..]);
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
        k += identifier_len(&self.source[k..]);
        self.push(id("T_STRING"), name, k);
        self.i = k;
    }

    /// Scans Zend's `ST_VAR_OFFSET` vocabulary until the closing bracket.
    #[inline(never)]
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
                k += identifier_len(&self.source[k..]);
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
                // Zend matches `{LNUM}|{HNUM}|{BNUM}|{ONUM}` only, then lexes the remaining
                // bytes as a label: `"$a[1e3]"` is T_NUM_STRING "1" + T_STRING "e3".
                let k = scan_offset_number(self.source, self.i);
                self.push(id("T_NUM_STRING"), number, k);
                self.i = k;
                first = false;
                continue;
            }
            if is_label_start(c) {
                let name = self.i;
                let mut k = self.i + 1;
                k += identifier_len(&self.source[k..]);
                self.push(id("T_STRING"), name, k);
                self.i = k;
                first = false;
                continue;
            }
            if matches!(c, b'\'' | b' ' | b'\t' | b'\r' | b'\n' | b'#' | b'\\') {
                self.errors.push(LexError {
                    pos: self.i,
                    message: "syntax error, unexpected string content \"\", expecting \"-\" or identifier or variable or number".into(),
                });
                self.push(id("T_ENCAPSED_AND_WHITESPACE"), self.i, self.i);
                return;
            }
            if is_bad_character(c) {
                const HEX: &[u8; 16] = b"0123456789ABCDEF";
                let mut message = String::with_capacity(72);
                message.push_str("syntax error, unexpected character 0x");
                message.push(HEX[(c >> 4) as usize] as char);
                message.push(HEX[(c & 0x0F) as usize] as char);
                message.push_str(if first {
                    ", expecting \"-\" or identifier or variable or number"
                } else {
                    ", expecting \"]\""
                });
                self.errors.push(LexError { pos: self.i, message });
                self.push(id("T_BAD_CHARACTER"), self.i, self.i + 1);
                self.i += 1;
                first = false;
                continue;
            }
            // Punctuation (`+`, `.`, `?`, `"`, `$` without a label, …) is a single byte
            // inside the offset grammar and the scan continues.
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
        j += identifier_len(&self.source[j..]);
        // Only a name body is `T_STRING_VARNAME`; `${123}` and `${1 + 2}` are expressions.
        if j > name_start
            && is_label_start(self.source[name_start])
            && matches!(self.source.get(j), Some(b'[' | b'}'))
        {
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

/// Reports PHP's octal-escape overflow warning for one double-quoted literal run.
///
/// Zend accepts `\0`..`\377` and warns for anything above one byte (`\400`..`\777`), in both
/// plain tokenizing and `TOKEN_PARSE`.
fn validate_octal_escapes(
    source: &[u8],
    start: usize,
    end: usize,
    warnings: &mut Vec<LexWarning>,
) {
    let mut i = start;
    while i < end {
        if source[i] != b'\\' {
            i += 1;
            continue;
        }
        if i + 1 >= end {
            break;
        }
        if !matches!(source[i + 1], b'0'..=b'7') {
            i += 2;
            continue;
        }
        let mut value = 0u32;
        let mut j = i + 1;
        let mut digits = 0usize;
        while j < end && digits < 3 && matches!(source[j], b'0'..=b'7') {
            value = value * 8 + u32::from(source[j] - b'0');
            j += 1;
            digits += 1;
        }
        if value > 0o377 {
            let text = String::from_utf8_lossy(&source[i..j]).into_owned();
            warnings.push(LexWarning {
                message: format!("Octal escape sequence overflow {text} is greater than \\377"),
                pos: i,
                severity: 2,
                parse_only: false,
            });
        }
        i = j;
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
    let mut lexer = Lexer::new(source, ());
    lexer.run();
    (lexer.tokens, lexer.errors, lexer.warnings)
}

/// Scans runtime source starting in PHP code for eval or HTML for included files.
pub(super) fn scan_runtime(source: &[u8], file: bool) -> (Vec<Token>, Vec<LexError>, Vec<LexWarning>) {
    let mode = if file { Mode::InlineHtml } else { Mode::Php };
    let mut lexer = Lexer { mode, runtime: true, number_profile: NumberProfile::Runtime, ..Lexer::new(source, ()) };
    lexer.run();
    (lexer.tokens, lexer.errors, lexer.warnings)
}

/// Scans compiler source and streams its syntax projection to the supplied output.
pub(crate) fn scan_compiler<C: CompilerOutput>(
    source: &[u8],
    requires_open_tag: bool,
    forbid_tags: bool,
    validation: bool,
    compiler: C,
) -> C {
    let number_profile = if validation { NumberProfile::Validation } else { NumberProfile::Compiler };
    let mut lexer = Lexer { mode: Mode::Php, forbid_tags, number_profile, ..Lexer::new(source, compiler) };
    if requires_open_tag {
        lexer.i = compiler_trivia_len(source);
        if !source[lexer.i..].starts_with(b"<?php") {
            lexer.emit_compiler(CompilerLexeme::MissingOpenTag, lexer.i);
            return lexer.compiler;
        }
    }
    let tag_end = lexer.i + if requires_open_tag { 5 } else { 0 };
    lexer.emit_compiler(CompilerLexeme::OpenTag, tag_end);
    lexer.run();
    lexer.compiler
}

/// Formats a PHP parser diagnostic for one unexpected token and an expected literal.
///
/// Numbers, variables, quoted strings and namespaced names use Zend's noun. Keywords use
/// their grammar spelling. A bare `"` is a double-quote mark. A short punctuation token keeps
/// the `token` noun. `T_EXIT` is always spelled `exit`.
fn unexpected_expecting(source: &[u8], token: &Token, expected: &str) -> String {
    let text = &source[token.pos..token.pos + token.len];
    if token.id == id("T_STRING") {
        format!("syntax error, unexpected identifier \"{}\", expecting {expected}", String::from_utf8_lossy(text))
    } else if token.id == id("T_EXIT") {
        // Zend names T_EXIT `exit` even when the source spells it `die`.
        format!("syntax error, unexpected token \"exit\", expecting {expected}")
    } else if token.id == id("T_LNUMBER") {
        format!("syntax error, unexpected integer \"{}\", expecting {expected}", String::from_utf8_lossy(text))
    } else if token.id == id("T_DNUMBER") {
        format!("syntax error, unexpected floating-point number \"{}\", expecting {expected}", String::from_utf8_lossy(text))
    } else if token.id == id("T_VARIABLE") {
        format!("syntax error, unexpected variable \"{}\", expecting {expected}", String::from_utf8_lossy(text))
    } else if token.id == id("T_CONSTANT_ENCAPSED_STRING") {
        let (noun, body) = quoted_string_noun(text);
        format!("syntax error, unexpected {noun} \"{}\", expecting {expected}", String::from_utf8_lossy(body))
    } else if token.id == id("T_NAME_QUALIFIED") {
        format!("syntax error, unexpected namespaced name \"{}\", expecting {expected}", String::from_utf8_lossy(text))
    } else if token.id == id("T_NAME_FULLY_QUALIFIED") {
        format!("syntax error, unexpected fully qualified name \"{}\", expecting {expected}", String::from_utf8_lossy(text))
    } else if token.id == id("T_NAME_RELATIVE") {
        format!("syntax error, unexpected namespace-relative name \"{}\", expecting {expected}", String::from_utf8_lossy(text))
    } else if text == b"\"" {
        format!("syntax error, unexpected double-quote mark, expecting {expected}")
    } else if token.id == id("T_START_HEREDOC") {
        let header = super::heredoc_start_header(text);
        format!(
            "syntax error, unexpected heredoc start \"{}\", expecting {expected}",
            String::from_utf8_lossy(header)
        )
    } else if let Some(cast) = super::cast_diagnostic_text(token.id) {
        format!("syntax error, unexpected token \"{cast}\", expecting {expected}")
    } else if token.id == id("T_CLOSE_TAG") {
        format!("syntax error, unexpected token \";\", expecting {expected}")
    } else if let Some(keyword) = super::keyword_diagnostic_spelling(token.id, text) {
        format!("syntax error, unexpected token \"{keyword}\", expecting {expected}")
    } else if text.len() <= 3 {
        format!("syntax error, unexpected token \"{}\", expecting {expected}", String::from_utf8_lossy(text))
    } else {
        format!(
            "syntax error, unexpected token \"{}\", expecting {expected}",
            super::token_name(token.id).unwrap_or("UNKNOWN")
        )
    }
}

/// Returns the byte PHP names for a halt-argument token.
///
/// A heredoc start is reported on the first body line, after the header newline. Every other
/// token keeps its own start.
fn diagnostic_pos(source: &[u8], token: &Token) -> usize {
    if token.id != id("T_START_HEREDOC") {
        return token.pos;
    }
    let text = &source[token.pos..token.pos + token.len];
    let header = text.split(|byte| *byte == b'\n' || *byte == b'\r').next().unwrap_or(text);
    let mut error_pos = token.pos + header.len();
    if text.get(header.len()) == Some(&b'\r') && text.get(header.len() + 1) == Some(&b'\n') {
        error_pos += 2;
    } else if text.get(header.len()).is_some() {
        error_pos += 1;
    }
    error_pos.min(token.pos + token.len)
}

/// Returns Zend's noun and the visible body of one quoted-string token.
fn quoted_string_noun(text: &[u8]) -> (&'static str, &[u8]) {
    match text.first() {
        Some(b'\'') => ("single-quoted string", &text[1..text.len().saturating_sub(1)]),
        Some(b'"') => ("double-quoted string", &text[1..text.len().saturating_sub(1)]),
        Some(b'b') | Some(b'B') if text.len() >= 3 => {
            ("quoted string", &text[..text.len().saturating_sub(1)])
        }
        _ => ("quoted string", text),
    }
}

/// Returns whether a token belongs to PHP's trivia set for halt-compiler accounting.
fn is_trivia(token_id: i64) -> bool {
    [id("T_WHITESPACE"), id("T_COMMENT"), id("T_DOC_COMMENT"), id("T_OPEN_TAG")].contains(&token_id)
}

/// Returns whether a token ID makes a following `__halt_compiler` an ordinary member/name.
///
/// `::` is deliberately absent: PHP 8.5 rejects `X::__halt_compiler` as a class-constant
/// name, while `$o->__halt_compiler` and `namespace\__halt_compiler` are valid names.
fn is_halt_name_separator(token_id: i64) -> bool {
    token_id == id("T_OBJECT_OPERATOR") || token_id == id("T_NULLSAFE_OBJECT_OPERATOR")
}

/// Returns whether a byte is a bad character in PHP code.
fn is_bad_character(byte: u8) -> bool {
    (byte < 0x20 && !matches!(byte, b'\t' | b'\n' | b'\r')) || byte == 0x7f
}

/// Finds one byte sequence inside a larger buffer.
fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|window| window == needle)
}

/// Skips whitespace and comments for PHP's contextual lookahead rules.
fn skip_trivia(source: &[u8], mut at: usize) -> usize {
    while let Some((_, len)) = php_trivia(&source[at..]) {
        at += len;
    }
    at
}

/// Scans the numeric prefix Zend accepts as a `T_NUM_STRING` offset inside interpolation.
#[inline(never)]
fn scan_offset_number(source: &[u8], start: usize) -> usize {
    if let Some(radix) = numeric_prefix(&source[start..], false) {
        let digits_start = start + 2;
        return digits_start + digits_len(&source[digits_start..], radix);
    }
    start + digits_len(&source[start..], 10)
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
