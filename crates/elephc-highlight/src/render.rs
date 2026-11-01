//! Purpose:
//! Renders PHP source exactly as `highlight_string()`/`highlight_file()` do: PHP's token classes
//! mapped onto the five colours of `zend_highlight`, with PHP's escaping and span coalescing.
//!
//! Called from:
//! - `crate::abi`, the C entry compiled programs call for `highlight_string()`/`highlight_file()`.
//! - The elephc-magician interpreter's `highlight_string()`/`highlight_file()` builtins, so both
//!   backends share one renderer.
//!
//! Key details:
//! - Output must match reference PHP byte for byte, so the scanner mirrors the Zend lexer's token
//!   boundaries: whitespace inherits the preceding token's colour, `//`/`#` comments stop at `?>`
//!   (which closes PHP), inline HTML carries no span, and interpolated strings mix literal (red),
//!   variable (blue) and brace/operator (green) runs.
//! - Only `&`, `<` and `>` are escaped; PHP leaves quotes and backslashes alone.
//! - A highlighter, not a parser: unterminated constructs consume the rest of the input, as the
//!   reference lexer reports them.

use std::collections::VecDeque;

/// Colours PHP's highlighter uses, by token class.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Class {
    /// Source outside every `<?php … ?>` block: printed without a span (the wrapper's own colour).
    InlineHtml,
    /// Open tag, close tag, variables, names, numbers, magic constants: `#0000BB`.
    Blue,
    /// Keywords, operators, punctuation, casts, heredoc markers: `#007700`.
    Green,
    /// Comments: `#FF8000`.
    Comment,
    /// String literals and their literal parts: `#DD0000`.
    Red,
}

impl Class {
    /// Returns the CSS colour of one token class, or `None` for the wrapper's default.
    fn colour(self) -> Option<&'static str> {
        match self {
            Self::InlineHtml => None,
            Self::Blue => Some("#0000BB"),
            Self::Green => Some("#007700"),
            Self::Comment => Some("#FF8000"),
            Self::Red => Some("#DD0000"),
        }
    }
}

/// Reserved words PHP's lexer emits as keyword tokens, lower-cased.
///
/// `self`, `parent`, `true`, `false`, `null`, the type names and the magic constants are
/// deliberately absent: the Zend lexer yields `T_STRING` or magic-constant tokens for them and
/// the highlighter paints those blue.
const KEYWORDS: &[&str] = &[
    "__halt_compiler", "abstract", "and", "array", "as", "break", "callable", "case", "catch",
    "class", "clone", "const", "continue", "declare", "default", "die", "do", "echo", "else",
    "elseif", "empty", "enddeclare", "endfor", "endforeach", "endif", "endswitch", "endwhile",
    "eval", "exit", "extends", "final", "finally", "fn", "for", "foreach", "function", "global",
    "goto", "if", "implements", "include", "include_once", "instanceof", "insteadof", "interface",
    "isset", "list", "match", "namespace", "new", "or", "print", "private", "protected", "public",
    "readonly", "require", "require_once", "return", "static", "switch", "throw", "trait", "try",
    "unset", "use", "var", "while", "xor", "yield",
];

/// Cast spellings the lexer turns into a single `T_*_CAST` token.
const CASTS: &[&str] = &[
    "int", "integer", "bool", "boolean", "float", "double", "real", "string", "binary", "array",
    "object", "unset",
];

/// Renders the inner HTML of `highlight_string($source, true)`, without the `<pre><code>` wrapper.
pub fn highlight_inner_html(source: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(source.len() + source.len() / 4);
    let mut scanner = Scanner::new(source);
    let mut open: Option<Class> = None;
    while let Some((class, start, end)) = scanner.next_token() {
        let text = &source[start..end];
        match class.colour() {
            None => {
                if open.take().is_some() {
                    out.extend_from_slice(b"</span>");
                }
                escape_into(&mut out, text);
            }
            Some(colour) => {
                if open != Some(class) {
                    if open.take().is_some() {
                        out.extend_from_slice(b"</span>");
                    }
                    out.extend_from_slice(b"<span style=\"color: ");
                    out.extend_from_slice(colour.as_bytes());
                    out.extend_from_slice(b"\">");
                    open = Some(class);
                }
                escape_into(&mut out, text);
            }
        }
    }
    if open.is_some() {
        out.extend_from_slice(b"</span>");
    }
    out
}

/// Wraps [`highlight_inner_html`] in PHP's `<pre><code style="color: #000000">` container.
pub fn highlight_document(source: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(source.len() + 64);
    out.extend_from_slice(b"<pre><code style=\"color: #000000\">");
    out.extend_from_slice(&highlight_inner_html(source));
    out.extend_from_slice(b"</code></pre>");
    out
}

/// Escapes one token the way the highlighter does: `&`, `<` and `>` change, and a tab becomes four
/// spaces (reference PHP expands tabs in highlighted output, including inside inline HTML).
fn escape_into(out: &mut Vec<u8>, text: &[u8]) {
    for &byte in text {
        match byte {
            b'&' => out.extend_from_slice(b"&amp;"),
            b'<' => out.extend_from_slice(b"&lt;"),
            b'>' => out.extend_from_slice(b"&gt;"),
            b'\t' => out.extend_from_slice(b"    "),
            other => out.push(other),
        }
    }
}

/// One pending coloured run.
type Run = (Class, usize, usize);

/// Byte scanner that yields `(class, start, end)` runs in source order.
struct Scanner<'a> {
    source: &'a [u8],
    pos: usize,
    /// Colour the previous token carried; whitespace joins it, exactly as the Zend highlighter does.
    last: Class,
    /// Whether the scanner is inside a `<?php … ?>` block.
    in_php: bool,
    /// Runs produced by a construct that spans several token classes, consumed before the next scan.
    pending: VecDeque<Run>,
}

impl<'a> Scanner<'a> {
    /// Creates a scanner that starts in inline-HTML mode like the Zend lexer.
    fn new(source: &'a [u8]) -> Self {
        Self { source, pos: 0, last: Class::InlineHtml, in_php: false, pending: VecDeque::new() }
    }

    /// Returns the next coloured run, or `None` at end of input.
    fn next_token(&mut self) -> Option<Run> {
        if let Some(run) = self.pending.pop_front() {
            return Some(run);
        }
        if self.pos >= self.source.len() {
            return None;
        }
        if self.in_php {
            self.scan_php_token()
        } else {
            self.scan_inline_html()
        }
    }

    /// Pushes one run while updating the inherited colour for the whitespace that follows it.
    fn push(&mut self, class: Class, start: usize, end: usize) {
        if start >= end {
            return;
        }
        self.pending.push_back((class, start, end));
        self.last = class;
    }

    /// Emits the HTML run before the next PHP open tag, then the tag itself.
    fn scan_inline_html(&mut self) -> Option<Run> {
        let start = self.pos;
        let mut cursor = self.pos;
        while cursor < self.source.len() {
            if self.source[cursor] == b'<' {
                if self.source[cursor..].starts_with(b"<?=") {
                    break;
                }
                if self.source.len() >= cursor + 5
                    && self.source[cursor..cursor + 5].eq_ignore_ascii_case(b"<?php")
                    && matches!(
                        self.source.get(cursor + 5),
                        None | Some(b' ') | Some(b'\t') | Some(b'\r') | Some(b'\n')
                    )
                {
                    break;
                }
            }
            cursor += 1;
        }
        if cursor > start {
            self.pos = cursor;
            self.pending.push_back((Class::InlineHtml, start, cursor));
            self.last = Class::InlineHtml;
            return self.next_token();
        }
        let length = if self.source[cursor..].starts_with(b"<?=") { 3 } else { 5 };
        self.pos = cursor + length;
        self.in_php = true;
        self.pending.push_back((Class::Blue, cursor, self.pos));
        self.last = Class::Blue;
        self.next_token()
    }

    /// Scans one token of PHP code and returns the first run it produced.
    fn scan_php_token(&mut self) -> Option<Run> {
        let start = self.pos;
        let byte = self.source[start];
        // Close tag, which ends PHP (and absorbs one trailing newline like the lexer's token).
        if byte == b'?' && self.source[start..].starts_with(b"?>") {
            self.pos = start + 2;
            if self.source.get(self.pos) == Some(&b'\r') && self.source.get(self.pos + 1) == Some(&b'\n') {
                self.pos += 2;
            } else if self.source.get(self.pos) == Some(&b'\n') {
                self.pos += 1;
            }
            self.in_php = false;
            self.push(Class::Blue, start, self.pos);
            return self.next_token();
        }
        // Whitespace inherits the previous token's colour.
        if matches!(byte, b' ' | b'\t' | b'\r' | b'\n') {
            let mut cursor = start;
            while matches!(
                self.source.get(cursor),
                Some(b' ') | Some(b'\t') | Some(b'\r') | Some(b'\n')
            ) {
                cursor += 1;
            }
            self.pos = cursor;
            let class = self.last;
            self.push(class, start, cursor);
            return self.next_token();
        }
        // `//` and `#` comments stop before `?>`; `/* … */` ends at its terminator.
        if (byte == b'/' && self.source.get(start + 1) == Some(&b'/'))
            || (byte == b'#' && self.source.get(start + 1) != Some(&b'['))
        {
            let mut cursor = start;
            while cursor < self.source.len() {
                if self.source[cursor] == b'\n' {
                    cursor += 1;
                    break;
                }
                if self.source[cursor] == b'?' && self.source[cursor..].starts_with(b"?>") {
                    break;
                }
                cursor += 1;
            }
            self.pos = cursor;
            self.push(Class::Comment, start, cursor);
            return self.next_token();
        }
        if byte == b'/' && self.source.get(start + 1) == Some(&b'*') {
            let mut cursor = start + 2;
            let mut closed = false;
            while cursor + 1 < self.source.len() {
                if self.source[cursor] == b'*' && self.source[cursor + 1] == b'/' {
                    cursor += 2;
                    closed = true;
                    break;
                }
                cursor += 1;
            }
            if !closed {
                cursor = self.source.len();
            }
            self.pos = cursor;
            self.push(Class::Comment, start, cursor);
            return self.next_token();
        }
        // `#[` is one green attribute token.
        if byte == b'#' && self.source.get(start + 1) == Some(&b'[') {
            self.pos = start + 2;
            self.push(Class::Green, start, start + 2);
            return self.next_token();
        }
        // Single-quoted string: escapes keep the quote inert.
        if byte == b'\'' {
            let end = self.scan_single_quoted(start);
            self.pos = end;
            self.push(Class::Red, start, end);
            return self.next_token();
        }
        // Double-quoted and backtick strings can interpolate.
        if byte == b'"' || byte == b'`' {
            let end = self.scan_interpolated(start, Some(byte), None);
            self.pos = end;
            return self.next_token();
        }
        // Heredoc / nowdoc.
        if self.source[start..].starts_with(b"<<<") {
            if let Some(end) = self.scan_heredoc(start) {
                self.pos = end;
                return self.next_token();
            }
        }
        // Variables.
        if byte == b'$' && self.source.get(start + 1).is_some_and(|next| is_name_start(*next)) {
            let mut cursor = start + 1;
            while self.source.get(cursor).is_some_and(|next| is_name_continue(*next)) {
                cursor += 1;
            }
            self.pos = cursor;
            self.push(Class::Blue, start, cursor);
            return self.next_token();
        }
        // Numbers.
        if byte.is_ascii_digit()
            || (byte == b'.' && self.source.get(start + 1).is_some_and(u8::is_ascii_digit))
        {
            let end = self.scan_number(start);
            self.pos = end;
            self.push(Class::Blue, start, end);
            return self.next_token();
        }
        // Names, including qualified and fully-qualified ones.
        if is_name_start(byte)
            || (byte == b'\\' && self.source.get(start + 1).is_some_and(|next| is_name_start(*next)))
        {
            let mut cursor = start;
            let mut qualified = byte == b'\\';
            if byte == b'\\' {
                cursor += 1;
            }
            loop {
                while self.source.get(cursor).is_some_and(|next| is_name_continue(*next)) {
                    cursor += 1;
                }
                if self.source.get(cursor) == Some(&b'\\')
                    && self.source.get(cursor + 1).is_some_and(|next| is_name_start(*next))
                {
                    qualified = true;
                    cursor += 1;
                    continue;
                }
                break;
            }
            let word = &self.source[start..cursor];
            let class = if !qualified && KEYWORDS.iter().any(|keyword| word.eq_ignore_ascii_case(keyword.as_bytes())) {
                Class::Green
            } else {
                Class::Blue
            };
            self.pos = cursor;
            self.push(class, start, cursor);
            return self.next_token();
        }
        // Casts: `(` whitespace word whitespace `)` as one green token.
        if byte == b'(' {
            if let Some(end) = self.scan_cast(start) {
                self.pos = end;
                self.push(Class::Green, start, end);
                return self.next_token();
            }
        }
        // Everything else is an operator or punctuation run, all painted green.
        let mut cursor = start + 1;
        while cursor < self.source.len() {
            let next = self.source[cursor];
            if matches!(next, b' ' | b'\t' | b'\r' | b'\n')
                || is_name_start(next)
                || next.is_ascii_digit()
                || next == b'$'
                || next == b'"'
                || next == b'\''
                || next == b'`'
                || next == b'#'
            {
                break;
            }
            if next == b'?' && self.source[cursor..].starts_with(b"?>") {
                break;
            }
            if next == b'/' && matches!(self.source.get(cursor + 1), Some(b'/') | Some(b'*')) {
                break;
            }
            // `\Throwable` is one name token, not a keyword run plus a name: the backslash the
            // scanner would otherwise leave in `catch (` belongs to the name's own default run.
            if next == b'\\'
                && self.source.get(cursor + 1).is_some_and(|next| is_name_start(*next))
            {
                break;
            }
            cursor += 1;
        }
        self.pos = cursor;
        self.push(Class::Green, start, cursor);
        self.next_token()
    }

    /// Scans one single-quoted string, honouring `\\` and `\'`.
    fn scan_single_quoted(&self, start: usize) -> usize {
        let mut cursor = start + 1;
        while cursor < self.source.len() {
            match self.source[cursor] {
                b'\\' => cursor += 2,
                b'\'' => return cursor + 1,
                _ => cursor += 1,
            }
        }
        self.source.len()
    }

    /// Scans a double-quoted, backtick, or heredoc body, pushing literal and interpolated runs.
    ///
    /// `quote` is the terminating byte for quoted strings, or `None` for a heredoc whose body ends
    /// at `body_end`. Returns the byte offset just past the construct.
    fn scan_interpolated(&mut self, start: usize, quote: Option<u8>, body_end: Option<usize>) -> usize {
        let mut cursor = start;
        let terminator = quote.map(|_| start + 1).unwrap_or(start);
        let mut literal_start = start;
        if quote.is_some() {
            cursor = terminator;
        }
        loop {
            let limit = body_end.unwrap_or(self.source.len());
            if cursor >= limit {
                self.push(Class::Red, literal_start, limit);
                return limit;
            }
            let byte = self.source[cursor];
            if let Some(quote_byte) = quote {
                if byte == b'\\' {
                    cursor = (cursor + 2).min(limit);
                    continue;
                }
                if byte == quote_byte {
                    self.push(Class::Red, literal_start, cursor + 1);
                    return cursor + 1;
                }
            }
            let interpolation = (byte == b'$' && self.source.get(cursor + 1).is_some_and(|next| is_name_start(*next)))
                || (byte == b'$' && self.source.get(cursor + 1) == Some(&b'{'))
                || (byte == b'{' && self.source.get(cursor + 1) == Some(&b'$'));
            if !interpolation {
                cursor += 1;
                continue;
            }
            // Literal text before the interpolation.
            self.push(Class::Red, literal_start, cursor);
            if byte == b'$' && self.source.get(cursor + 1) == Some(&b'{') {
                self.push(Class::Green, cursor, cursor + 2);
                cursor = self.scan_braced_expression(cursor + 2);
                literal_start = cursor;
                continue;
            }
            if byte == b'{' {
                self.push(Class::Green, cursor, cursor + 1);
                cursor = self.scan_braced_expression(cursor + 1);
                literal_start = cursor;
                continue;
            }
            // `$name` with an optional single offset or property access.
            let mut name_end = cursor + 1;
            while self.source.get(name_end).is_some_and(|next| is_name_continue(*next)) {
                name_end += 1;
            }
            self.push(Class::Blue, cursor, name_end);
            cursor = name_end;
            if self.source.get(cursor) == Some(&b'[') {
                self.push(Class::Green, cursor, cursor + 1);
                cursor += 1;
                while cursor < limit && self.source[cursor] != b']' {
                    if self.source[cursor] == b'-' || self.source[cursor].is_ascii_digit() {
                        let number_end = self.scan_number(cursor);
                        self.push(Class::Blue, cursor, number_end);
                        cursor = number_end;
                        continue;
                    }
                    if is_name_start(self.source[cursor]) {
                        let mut word_end = cursor + 1;
                        while self.source.get(word_end).is_some_and(|next| is_name_continue(*next)) {
                            word_end += 1;
                        }
                        self.push(Class::Blue, cursor, word_end);
                        cursor = word_end;
                        continue;
                    }
                    cursor += 1;
                }
                if cursor < limit {
                    self.push(Class::Green, cursor, cursor + 1);
                    cursor += 1;
                }
            } else if self.source.get(cursor) == Some(&b'-') && self.source.get(cursor + 1) == Some(&b'>') {
                self.push(Class::Green, cursor, cursor + 2);
                cursor += 2;
                let word_start = cursor;
                while self.source.get(cursor).is_some_and(|next| is_name_continue(*next)) {
                    cursor += 1;
                }
                self.push(Class::Blue, word_start, cursor);
            }
            literal_start = cursor;
        }
    }

    /// Scans a `{…}`/`${…}` interpolated expression, returning the offset just past its `}`.
    ///
    /// The inner tokens are ordinary PHP code, so this reuses the main scanner with a brace-depth
    /// guard: a `}` at depth zero closes the interpolation and is painted green like punctuation.
    fn scan_braced_expression(&mut self, mut cursor: usize) -> usize {
        let mut depth = 0usize;
        while cursor < self.source.len() {
            let byte = self.source[cursor];
            if byte == b'}' {
                if depth == 0 {
                    self.push(Class::Green, cursor, cursor + 1);
                    return cursor + 1;
                }
                depth -= 1;
                self.push(Class::Green, cursor, cursor + 1);
                cursor += 1;
                continue;
            }
            if byte == b'{' {
                depth += 1;
                self.push(Class::Green, cursor, cursor + 1);
                cursor += 1;
                continue;
            }
            if byte == b'"' || byte == b'`' {
                let end = self.scan_interpolated(cursor, Some(byte), None);
                cursor = end;
                continue;
            }
            if byte == b'\'' {
                let end = self.scan_single_quoted(cursor);
                self.push(Class::Red, cursor, end);
                cursor = end;
                continue;
            }
            if byte == b'$' && self.source.get(cursor + 1).is_some_and(|next| is_name_start(*next)) {
                let mut end = cursor + 1;
                while self.source.get(end).is_some_and(|next| is_name_continue(*next)) {
                    end += 1;
                }
                self.push(Class::Blue, cursor, end);
                cursor = end;
                continue;
            }
            if is_name_start(byte) {
                let mut end = cursor + 1;
                while self.source.get(end).is_some_and(|next| is_name_continue(*next)) {
                    end += 1;
                }
                let word = &self.source[cursor..end];
                let class = if KEYWORDS.iter().any(|keyword| word.eq_ignore_ascii_case(keyword.as_bytes())) {
                    Class::Green
                } else {
                    Class::Blue
                };
                self.push(class, cursor, end);
                cursor = end;
                continue;
            }
            if byte.is_ascii_digit() {
                let end = self.scan_number(cursor);
                self.push(Class::Blue, cursor, end);
                cursor = end;
                continue;
            }
            // Operators and punctuation, one byte at a time so a closing `}` is never swallowed.
            self.push(Class::Green, cursor, cursor + 1);
            cursor += 1;
        }
        cursor
    }

    /// Scans a heredoc or nowdoc construct, pushing its opener, body and closer runs.
    fn scan_heredoc(&mut self, start: usize) -> Option<usize> {
        let mut cursor = start + 3;
        while matches!(self.source.get(cursor), Some(b' ') | Some(b'\t')) {
            cursor += 1;
        }
        let quote = match self.source.get(cursor) {
            Some(b'\'') | Some(b'"') => Some(self.source[cursor]),
            _ => None,
        };
        if quote.is_some() {
            cursor += 1;
        }
        let label_start = cursor;
        while self.source.get(cursor).is_some_and(|next| is_name_continue(*next)) {
            cursor += 1;
        }
        if cursor == label_start {
            return None;
        }
        let label = self.source[label_start..cursor].to_vec();
        if let Some(quote_byte) = quote {
            if self.source.get(cursor) != Some(&quote_byte) {
                return None;
            }
            cursor += 1;
        }
        while matches!(self.source.get(cursor), Some(b' ') | Some(b'\t') | Some(b'\r')) {
            cursor += 1;
        }
        if self.source.get(cursor) != Some(&b'\n') {
            return None;
        }
        let body_start = cursor + 1;
        // The body ends at the label alone on its line, leading indentation allowed.
        let mut line_start = body_start;
        let close_start;
        loop {
            if line_start >= self.source.len() {
                close_start = self.source.len();
                break;
            }
            let mut probe = line_start;
            while matches!(self.source.get(probe), Some(b' ') | Some(b'\t')) {
                probe += 1;
            }
            if self.source[probe..].starts_with(&label)
                && !self.source.get(probe + label.len()).is_some_and(|next| is_name_continue(*next))
            {
                close_start = line_start;
                break;
            }
            match self.source[line_start..].iter().position(|byte| *byte == b'\n') {
                Some(offset) => line_start += offset + 1,
                None => {
                    close_start = self.source.len();
                    break;
                }
            }
        }
        self.push(Class::Green, start, body_start);
        if quote.is_none() {
            self.scan_interpolated(body_start, None, Some(close_start));
        } else {
            self.push(Class::Red, body_start, close_start);
        }
        // The closing label, with its indentation and any trailing `;`/newline, is green.
        let mut close_end = close_start;
        while self.source.get(close_end).is_some_and(|next| matches!(next, b' ' | b'\t')) {
            close_end += 1;
        }
        close_end = (close_end + label.len()).min(self.source.len());
        self.push(Class::Green, close_start, close_end);
        Some(close_end)
    }

    /// Scans one number literal: decimal, hex, binary, octal, or float.
    fn scan_number(&self, start: usize) -> usize {
        let mut cursor = start;
        if self.source[cursor] == b'0' {
            match self.source.get(cursor + 1) {
                Some(b'x') | Some(b'X') => {
                    cursor += 2;
                    while self.source.get(cursor).is_some_and(|next| next.is_ascii_hexdigit() || *next == b'_') {
                        cursor += 1;
                    }
                    return cursor;
                }
                Some(b'b') | Some(b'B') => {
                    cursor += 2;
                    while self.source.get(cursor).is_some_and(|next| matches!(next, b'0' | b'1' | b'_')) {
                        cursor += 1;
                    }
                    return cursor;
                }
                Some(b'o') | Some(b'O') => {
                    cursor += 2;
                    while self.source.get(cursor).is_some_and(|next| matches!(next, b'0'..=b'7' | b'_')) {
                        cursor += 1;
                    }
                    return cursor;
                }
                _ => {}
            }
        }
        while self.source.get(cursor).is_some_and(|next| next.is_ascii_digit() || *next == b'_') {
            cursor += 1;
        }
        if self.source.get(cursor) == Some(&b'.')
            && self.source.get(cursor + 1).is_some_and(u8::is_ascii_digit)
        {
            cursor += 1;
            while self.source.get(cursor).is_some_and(|next| next.is_ascii_digit() || *next == b'_') {
                cursor += 1;
            }
        }
        if matches!(self.source.get(cursor), Some(b'e') | Some(b'E')) {
            let mut exponent = cursor + 1;
            if matches!(self.source.get(exponent), Some(b'+') | Some(b'-')) {
                exponent += 1;
            }
            if self.source.get(exponent).is_some_and(u8::is_ascii_digit) {
                cursor = exponent;
                while self.source.get(cursor).is_some_and(|next| next.is_ascii_digit() || *next == b'_') {
                    cursor += 1;
                }
            }
        }
        cursor
    }

    /// Recognises a `(int)`-style cast token and returns its end, or `None`.
    fn scan_cast(&self, start: usize) -> Option<usize> {
        let mut cursor = start + 1;
        while matches!(self.source.get(cursor), Some(b' ') | Some(b'\t')) {
            cursor += 1;
        }
        let word_start = cursor;
        while self.source.get(cursor).is_some_and(u8::is_ascii_alphabetic) {
            cursor += 1;
        }
        let word = &self.source[word_start..cursor];
        if !CASTS.iter().any(|cast| word.eq_ignore_ascii_case(cast.as_bytes())) {
            return None;
        }
        while matches!(self.source.get(cursor), Some(b' ') | Some(b'\t')) {
            cursor += 1;
        }
        (self.source.get(cursor) == Some(&b')')).then_some(cursor + 1)
    }
}

/// Returns whether one byte can start a PHP name.
fn is_name_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_' || byte >= 0x80
}

/// Returns whether one byte can continue a PHP name.
fn is_name_continue(byte: u8) -> bool {
    is_name_start(byte) || byte.is_ascii_digit()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Renders one source and returns it as UTF-8.
    fn rendered(source: &str) -> String {
        String::from_utf8(highlight_document(source.as_bytes())).expect("utf-8 highlight")
    }

    /// Oracle: `php -r 'echo highlight_string(...);'`.
    #[test]
    fn matches_php_for_html() {
        assert_eq!(rendered(r"just html & <tags>"), r#"<pre><code style="color: #000000">just html &amp; &lt;tags&gt;</code></pre>"#);
    }

    /// Oracle: `php -r 'echo highlight_string(...);'`.
    #[test]
    fn matches_php_for_basic() {
        assert_eq!(rendered(r"<?php
$a = 1; // c
"), r#"<pre><code style="color: #000000"><span style="color: #0000BB">&lt;?php
$a </span><span style="color: #007700">= </span><span style="color: #0000BB">1</span><span style="color: #007700">; </span><span style="color: #FF8000">// c
</span></code></pre>"#);
    }

    /// Oracle: `php -r 'echo highlight_string(...);'`.
    #[test]
    fn matches_php_for_echo_tag() {
        assert_eq!(rendered(r"x<?= $a ?>y"), r#"<pre><code style="color: #000000">x<span style="color: #0000BB">&lt;?= $a ?&gt;</span>y</code></pre>"#);
    }

    /// Oracle: `php -r 'echo highlight_string(...);'`.
    #[test]
    fn matches_php_for_close() {
        assert_eq!(rendered(r"<?php echo 1; ?>tail"), r#"<pre><code style="color: #000000"><span style="color: #0000BB">&lt;?php </span><span style="color: #007700">echo </span><span style="color: #0000BB">1</span><span style="color: #007700">; </span><span style="color: #0000BB">?&gt;</span>tail</code></pre>"#);
    }

    /// Oracle: `php -r 'echo highlight_string(...);'`.
    #[test]
    fn matches_php_for_hash_comment() {
        assert_eq!(rendered(r"<?php # hash
$a;"), r#"<pre><code style="color: #000000"><span style="color: #0000BB">&lt;?php </span><span style="color: #FF8000"># hash
</span><span style="color: #0000BB">$a</span><span style="color: #007700">;</span></code></pre>"#);
    }

    /// Oracle: `php -r 'echo highlight_string(...);'`.
    #[test]
    fn matches_php_for_doc() {
        assert_eq!(rendered(r"<?php
/** doc */
function f() {}"), r#"<pre><code style="color: #000000"><span style="color: #0000BB">&lt;?php
</span><span style="color: #FF8000">/** doc */
</span><span style="color: #007700">function </span><span style="color: #0000BB">f</span><span style="color: #007700">() {}</span></code></pre>"#);
    }

    /// Oracle: `php -r 'echo highlight_string(...);'`.
    #[test]
    fn matches_php_for_interp() {
        assert_eq!(rendered(r#"<?php $s = "a $b {$c} d";"#), r#"<pre><code style="color: #000000"><span style="color: #0000BB">&lt;?php $s </span><span style="color: #007700">= </span><span style="color: #DD0000">"a </span><span style="color: #0000BB">$b</span><span style="color: #DD0000"> </span><span style="color: #007700">{</span><span style="color: #0000BB">$c</span><span style="color: #007700">}</span><span style="color: #DD0000"> d"</span><span style="color: #007700">;</span></code></pre>"#);
    }

    /// Oracle: `php -r 'echo highlight_string(...);'`.
    #[test]
    fn matches_php_for_heredoc() {
        assert_eq!(rendered(r"<?php
$s = <<<EOT
line $x
EOT;
"), r#"<pre><code style="color: #000000"><span style="color: #0000BB">&lt;?php
$s </span><span style="color: #007700">= &lt;&lt;&lt;EOT
</span><span style="color: #DD0000">line </span><span style="color: #0000BB">$x</span><span style="color: #DD0000">
</span><span style="color: #007700">EOT;
</span></code></pre>"#);
    }

    /// Oracle: `php -r 'echo highlight_string(...);'`.
    #[test]
    fn matches_php_for_nowdoc() {
        assert_eq!(rendered(r"<?php
$s = <<<'EOT'
raw $x
EOT;
"), r#"<pre><code style="color: #000000"><span style="color: #0000BB">&lt;?php
$s </span><span style="color: #007700">= &lt;&lt;&lt;'EOT'
</span><span style="color: #DD0000">raw $x
</span><span style="color: #007700">EOT;
</span></code></pre>"#);
    }

    /// Oracle: `php -r 'echo highlight_string(...);'`.
    #[test]
    fn matches_php_for_numbers() {
        assert_eq!(rendered(r"<?php
$a = 0x1F + 0b101 + 0777 + 1.5e3 + 42;"), r#"<pre><code style="color: #000000"><span style="color: #0000BB">&lt;?php
$a </span><span style="color: #007700">= </span><span style="color: #0000BB">0x1F </span><span style="color: #007700">+ </span><span style="color: #0000BB">0b101 </span><span style="color: #007700">+ </span><span style="color: #0000BB">0777 </span><span style="color: #007700">+ </span><span style="color: #0000BB">1.5e3 </span><span style="color: #007700">+ </span><span style="color: #0000BB">42</span><span style="color: #007700">;</span></code></pre>"#);
    }

    /// Oracle: `php -r 'echo highlight_string(...);'`.
    #[test]
    fn matches_php_for_cast() {
        assert_eq!(rendered(r"<?php
$a = (int) $b;"), r#"<pre><code style="color: #000000"><span style="color: #0000BB">&lt;?php
$a </span><span style="color: #007700">= (int) </span><span style="color: #0000BB">$b</span><span style="color: #007700">;</span></code></pre>"#);
    }

    /// Oracle: `php -r 'echo highlight_string(...);'`.
    #[test]
    fn matches_php_for_attr() {
        assert_eq!(rendered(r"<?php
#[Attr(1)]
class C {}"), r#"<pre><code style="color: #000000"><span style="color: #0000BB">&lt;?php
</span><span style="color: #007700">#[</span><span style="color: #0000BB">Attr</span><span style="color: #007700">(</span><span style="color: #0000BB">1</span><span style="color: #007700">)]
class </span><span style="color: #0000BB">C </span><span style="color: #007700">{}</span></code></pre>"#);
    }

    /// Oracle: `php -r 'echo highlight_string(...);'`.
    #[test]
    fn matches_php_for_names() {
        assert_eq!(rendered(r"<?php
namespace A\B;
use C\D;
new \E\F();"), r#"<pre><code style="color: #000000"><span style="color: #0000BB">&lt;?php
</span><span style="color: #007700">namespace </span><span style="color: #0000BB">A\B</span><span style="color: #007700">;
use </span><span style="color: #0000BB">C\D</span><span style="color: #007700">;
new </span><span style="color: #0000BB">\E\F</span><span style="color: #007700">();</span></code></pre>"#);
    }

    /// Oracle: `php -r 'echo highlight_string(...);'`.
    ///
    /// A fully-qualified name that follows punctuation (`catch (\Throwable`) belongs to the
    /// name's default run, not to the keyword run that precedes it.
    #[test]
    fn matches_php_for_fully_qualified_names_after_punctuation() {
        assert_eq!(rendered(r#"<?php
try {} catch (\Throwable $e) { if (\function_exists("x")) {} }
"#), r#"<pre><code style="color: #000000"><span style="color: #0000BB">&lt;?php
</span><span style="color: #007700">try {} catch (</span><span style="color: #0000BB">\Throwable $e</span><span style="color: #007700">) { if (</span><span style="color: #0000BB">\function_exists</span><span style="color: #007700">(</span><span style="color: #DD0000">"x"</span><span style="color: #007700">)) {} }
</span></code></pre>"#);
    }

    /// Oracle: `php -r 'echo highlight_string(...);'`.
    #[test]
    fn matches_php_for_special() {
        assert_eq!(rendered(r"<?php
$a = $b <=> $c ?? $d ?: $e;"), r#"<pre><code style="color: #000000"><span style="color: #0000BB">&lt;?php
$a </span><span style="color: #007700">= </span><span style="color: #0000BB">$b </span><span style="color: #007700">&lt;=&gt; </span><span style="color: #0000BB">$c </span><span style="color: #007700">?? </span><span style="color: #0000BB">$d </span><span style="color: #007700">?: </span><span style="color: #0000BB">$e</span><span style="color: #007700">;</span></code></pre>"#);
    }

    /// Oracle: `php -r 'echo highlight_string(...);'`.
    #[test]
    fn matches_php_for_quotes() {
        assert_eq!(rendered(r#"<?php $a = "q\"x"; $b = '<&';"#), r#"<pre><code style="color: #000000"><span style="color: #0000BB">&lt;?php $a </span><span style="color: #007700">= </span><span style="color: #DD0000">"q\"x"</span><span style="color: #007700">; </span><span style="color: #0000BB">$b </span><span style="color: #007700">= </span><span style="color: #DD0000">'&lt;&amp;'</span><span style="color: #007700">;</span></code></pre>"#);
    }

    /// Oracle: `php -r 'echo highlight_string(...);'`.
    #[test]
    fn matches_php_for_empty() {
        assert_eq!(rendered(r""), r#"<pre><code style="color: #000000"></code></pre>"#);
    }

    /// Oracle: `php -r 'echo highlight_string(...);'`.
    #[test]
    fn matches_php_for_nophp() {
        assert_eq!(rendered(r"abc"), r#"<pre><code style="color: #000000">abc</code></pre>"#);
    }

    /// Oracle: `php -r 'echo highlight_string(...);'`.
    #[test]
    fn matches_php_for_quote_html() {
        assert_eq!(rendered(r#"a"b'c"#), r#"<pre><code style="color: #000000">a"b'c</code></pre>"#);
    }

    /// Oracle: `php -r 'echo highlight_string(...);'`.
    #[test]
    fn matches_php_for_tab() {
        assert_eq!(rendered(r"<?php	echo	1;"), r#"<pre><code style="color: #000000"><span style="color: #0000BB">&lt;?php    </span><span style="color: #007700">echo    </span><span style="color: #0000BB">1</span><span style="color: #007700">;</span></code></pre>"#);
    }

    /// Oracle: `php -r 'echo highlight_string(...);'`.
    #[test]
    fn matches_php_for_crlf() {
        assert_eq!(rendered(r"<?php
echo 1;
"), r#"<pre><code style="color: #000000"><span style="color: #0000BB">&lt;?php
</span><span style="color: #007700">echo </span><span style="color: #0000BB">1</span><span style="color: #007700">;
</span></code></pre>"#);
    }

    /// Oracle: `php -r 'echo highlight_string(...);'`.
    #[test]
    fn matches_php_for_consts() {
        assert_eq!(rendered(r"<?php
$a = true; $b = FALSE; $c = null;"), r#"<pre><code style="color: #000000"><span style="color: #0000BB">&lt;?php
$a </span><span style="color: #007700">= </span><span style="color: #0000BB">true</span><span style="color: #007700">; </span><span style="color: #0000BB">$b </span><span style="color: #007700">= </span><span style="color: #0000BB">FALSE</span><span style="color: #007700">; </span><span style="color: #0000BB">$c </span><span style="color: #007700">= </span><span style="color: #0000BB">null</span><span style="color: #007700">;</span></code></pre>"#);
    }

    /// Oracle: `php -r 'echo highlight_string(...);'`.
    #[test]
    fn matches_php_for_self() {
        assert_eq!(rendered(r"<?php
class A { function f() { return self::class; } }"), r#"<pre><code style="color: #000000"><span style="color: #0000BB">&lt;?php
</span><span style="color: #007700">class </span><span style="color: #0000BB">A </span><span style="color: #007700">{ function </span><span style="color: #0000BB">f</span><span style="color: #007700">() { return </span><span style="color: #0000BB">self</span><span style="color: #007700">::class; } }</span></code></pre>"#);
    }

    /// Oracle: `php -r 'echo highlight_string(...);'`.
    #[test]
    fn matches_php_for_arrow() {
        assert_eq!(rendered(r"<?php
$f = fn($x) => $x + 1;"), r#"<pre><code style="color: #000000"><span style="color: #0000BB">&lt;?php
$f </span><span style="color: #007700">= fn(</span><span style="color: #0000BB">$x</span><span style="color: #007700">) =&gt; </span><span style="color: #0000BB">$x </span><span style="color: #007700">+ </span><span style="color: #0000BB">1</span><span style="color: #007700">;</span></code></pre>"#);
    }

    /// Oracle: `php -r 'echo highlight_string(...);'`.
    #[test]
    fn matches_php_for_match() {
        assert_eq!(rendered(r"<?php
$r = match($x) { 1 => 'a', default => 'b' };"), r#"<pre><code style="color: #000000"><span style="color: #0000BB">&lt;?php
$r </span><span style="color: #007700">= match(</span><span style="color: #0000BB">$x</span><span style="color: #007700">) { </span><span style="color: #0000BB">1 </span><span style="color: #007700">=&gt; </span><span style="color: #DD0000">'a'</span><span style="color: #007700">, default =&gt; </span><span style="color: #DD0000">'b' </span><span style="color: #007700">};</span></code></pre>"#);
    }

    /// Oracle: `php -r 'echo highlight_string(...);'`.
    #[test]
    fn matches_php_for_interp_curly() {
        assert_eq!(rendered(r#"<?php $s = "pre {$a["k"]} post";"#), r#"<pre><code style="color: #000000"><span style="color: #0000BB">&lt;?php $s </span><span style="color: #007700">= </span><span style="color: #DD0000">"pre </span><span style="color: #007700">{</span><span style="color: #0000BB">$a</span><span style="color: #007700">[</span><span style="color: #DD0000">"k"</span><span style="color: #007700">]}</span><span style="color: #DD0000"> post"</span><span style="color: #007700">;</span></code></pre>"#);
    }

    /// Oracle: `php -r 'echo highlight_string(...);'`.
    #[test]
    fn matches_php_for_interp_dollar() {
        assert_eq!(rendered(r#"<?php $s = "$a[0] and $b->c";"#), r#"<pre><code style="color: #000000"><span style="color: #0000BB">&lt;?php $s </span><span style="color: #007700">= </span><span style="color: #DD0000">"</span><span style="color: #0000BB">$a</span><span style="color: #007700">[</span><span style="color: #0000BB">0</span><span style="color: #007700">]</span><span style="color: #DD0000"> and </span><span style="color: #0000BB">$b</span><span style="color: #007700">-&gt;</span><span style="color: #0000BB">c</span><span style="color: #DD0000">"</span><span style="color: #007700">;</span></code></pre>"#);
    }

    /// Oracle: `php -r 'echo highlight_string(...);'`.
    #[test]
    fn matches_php_for_close_in_comment() {
        assert_eq!(rendered(r"<?php // ?> still comment
$a;"), r#"<pre><code style="color: #000000"><span style="color: #0000BB">&lt;?php </span><span style="color: #FF8000">// </span><span style="color: #0000BB">?&gt;</span> still comment
$a;</code></pre>"#);
    }

    /// Oracle: `php -r 'echo highlight_string(...);'`.
    #[test]
    fn matches_php_for_close_in_string() {
        assert_eq!(rendered(r"<?php $a = '?>'; $b;"), r#"<pre><code style="color: #000000"><span style="color: #0000BB">&lt;?php $a </span><span style="color: #007700">= </span><span style="color: #DD0000">'?&gt;'</span><span style="color: #007700">; </span><span style="color: #0000BB">$b</span><span style="color: #007700">;</span></code></pre>"#);
    }

    /// Oracle: `php -r 'echo highlight_string(...);'`.
    #[test]
    fn matches_php_for_halt() {
        assert_eq!(rendered(r"<?php $a = 1; __halt_compiler(); raw data"), r#"<pre><code style="color: #000000"><span style="color: #0000BB">&lt;?php $a </span><span style="color: #007700">= </span><span style="color: #0000BB">1</span><span style="color: #007700">; __halt_compiler(); </span><span style="color: #0000BB">raw data</span></code></pre>"#);
    }

    /// Oracle: `php -r 'echo highlight_string(...);'`.
    #[test]
    fn matches_php_for_heredoc_indent() {
        assert_eq!(rendered(r"<?php
$s = <<<EOT
    a $x
    EOT;
"), r#"<pre><code style="color: #000000"><span style="color: #0000BB">&lt;?php
$s </span><span style="color: #007700">= &lt;&lt;&lt;EOT
</span><span style="color: #DD0000">    a </span><span style="color: #0000BB">$x</span><span style="color: #DD0000">
</span><span style="color: #007700">    EOT;
</span></code></pre>"#);
    }

    /// Oracle: `php -r 'echo highlight_string(...);'`.
    #[test]
    fn matches_php_for_attribute_str() {
        assert_eq!(rendered(r"<?php
#[Route('/a', name: 'x')]
class C {}"), r#"<pre><code style="color: #000000"><span style="color: #0000BB">&lt;?php
</span><span style="color: #007700">#[</span><span style="color: #0000BB">Route</span><span style="color: #007700">(</span><span style="color: #DD0000">'/a'</span><span style="color: #007700">, </span><span style="color: #0000BB">name</span><span style="color: #007700">: </span><span style="color: #DD0000">'x'</span><span style="color: #007700">)]
class </span><span style="color: #0000BB">C </span><span style="color: #007700">{}</span></code></pre>"#);
    }

    /// Oracle: `php -r 'echo highlight_string(...);'`.
    #[test]
    fn matches_php_for_phpdoc_types() {
        assert_eq!(rendered(r"<?php
/** @param int $a */
function f($a) {}"), r#"<pre><code style="color: #000000"><span style="color: #0000BB">&lt;?php
</span><span style="color: #FF8000">/** @param int $a */
</span><span style="color: #007700">function </span><span style="color: #0000BB">f</span><span style="color: #007700">(</span><span style="color: #0000BB">$a</span><span style="color: #007700">) {}</span></code></pre>"#);
    }

}