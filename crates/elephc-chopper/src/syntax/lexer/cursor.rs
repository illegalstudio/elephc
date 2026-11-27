//! Purpose:
//! Provides byte-level source traversal while tracking line and column spans.
//! Offers small peek/advance helpers used by lexer scanners.
//!
//! Called from:
//! - `crate::syntax::lexer::scan` and `crate::syntax::lexer::literals` scanners.
//!
//! Key details:
//! - Span positions are one-based and advance through newlines so diagnostics map back to PHP source.

use crate::syntax::span::Span;

/// Lexer cursor for source tracking.
pub struct Cursor<'a> {
    bytes: &'a [u8],
    /// The same source as `&str`, retained so `remaining()` can slice it in O(1).
    ///
    /// `None` only for the test-only byte constructor, whose input may be malformed. See
    /// `remaining` for why holding this matters.
    text: Option<&'a str>,
    pos: usize,
    line: usize,
    col: usize,
    source_id: u32,
    pub(super) semantic_profile: elephc_semantics::SemanticProfile,
}

impl<'a> Cursor<'a> {
    /// Constructs a cursor whose spans identify one included physical source file.
    pub(crate) fn new_in_source(
        source: &'a str,
        source_id: u32,
        semantic_profile: elephc_semantics::SemanticProfile,
    ) -> Self {
        Self {
            bytes: source.as_bytes(),
            text: Some(source),
            pos: 0,
            line: 1,
            col: 1,
            source_id,
            semantic_profile,
        }
    }

    /// Test-only constructor over raw bytes.
    ///
    /// The public `new` takes `&str` (always valid UTF-8), so it cannot exercise the
    /// malformed-byte handling in `peek`/`advance`. This builds a cursor directly over
    /// arbitrary bytes for those tests.
    #[cfg(test)]
    fn from_bytes(bytes: &'a [u8]) -> Self {
        Self {
            bytes,
            text: std::str::from_utf8(bytes).ok(),
            pos: 0,
            line: 1,
            col: 1,
            source_id: 0,
            semantic_profile: elephc_semantics::SemanticProfile::PERMISSIVE,
        }
    }

    /// Returns the current source position as a one-based `Span` (line, column).
    ///
    /// Used to attach source locations to tokens for diagnostics.
    pub fn span(&self) -> Span {
        Span::new_in_source(self.line as u32, self.col as u32, self.source_id)
    }

    /// Returns the next character without advancing the cursor.
    #[inline]
    pub fn peek(&self) -> Option<char> {
        if self.pos >= self.bytes.len() {
            return None;
        }
        let b = self.bytes[self.pos];
        if b.is_ascii() {
            Some(b as char)
        } else {
            // Decode the next full UTF-8 codepoint. On a malformed byte yield the U+FFFD
            // replacement char rather than None, so callers always observe a character
            // while bytes remain and a scan loop can never spin on a non-advancing None.
            self.remaining().chars().next().or(Some('\u{FFFD}'))
        }
    }

    /// Consumes and returns the next UTF-8 character, updating line and column counters.
    ///
    /// On newline (`\n`), column resets to 1 and line increments. Otherwise column increments.
    /// Returns `None` when the end of source has been reached.
    pub fn advance(&mut self) -> Option<char> {
        if self.pos >= self.bytes.len() {
            return None;
        }
        let b = self.bytes[self.pos];
        let ch = if b.is_ascii() {
            self.pos += 1;
            b as char
        } else {
            match self.remaining().chars().next() {
                Some(ch) => {
                    self.pos += ch.len_utf8();
                    ch
                }
                None => {
                    // Malformed byte: consume exactly one byte and report U+FFFD so the
                    // cursor always makes forward progress instead of returning a
                    // non-advancing None that a scan loop could spin on.
                    self.pos += 1;
                    '\u{FFFD}'
                }
            }
        };
        if ch == '\n' {
            self.line += 1;
            self.col = 1;
        } else {
            self.col += 1;
        }
        Some(ch)
    }

    /// Consumes known ASCII bytes on the current line.
    #[inline]
    pub(super) fn skip_ascii(&mut self, len: usize) {
        self.pos += len;
        self.col += len;
    }

    /// Consumes a UTF-8 slice identified by a byte scanner and updates the source span.
    #[inline]
    pub(crate) fn skip_bytes(&mut self, len: usize) {
        if len == 0 {
            return;
        }
        if len == 1 && self.bytes[self.pos] != b'\n' {
            self.skip_ascii(1);
            return;
        }
        let consumed = &self.remaining()[..len];
        let bytes = consumed.as_bytes();
        if bytes.is_ascii() {
            if let Some(last_newline) = bytes.iter().rposition(|byte| *byte == b'\n') {
                self.line += bytes.iter().filter(|byte| **byte == b'\n').count();
                self.col = len - last_newline;
            } else {
                self.col += len;
            }
            self.pos += len;
            return;
        }
        if let Some(last_newline) = consumed.rfind('\n') {
            self.line += bytes.iter().filter(|&&byte| byte == b'\n').count();
            self.col = consumed[last_newline + 1..].chars().count() + 1;
        } else {
            self.col += consumed.chars().count();
        }
        self.pos += len;
    }

    /// Returns the consumed source offset so a literal decoder can resume the scanner.
    pub(crate) fn position(&self) -> usize {
        self.pos
    }

    /// Returns true if the cursor has reached the end of source.
    pub fn is_eof(&self) -> bool {
        self.pos >= self.bytes.len()
    }

    /// Returns the slice of source from the current position to the end, as a `&str`.
    ///
    /// Projection and literal decoding call this for each lexeme and escape.
    /// Slicing the original validated `&str` keeps each lookup constant time.
    ///
    /// The public constructor takes `&str`, so the source is valid UTF-8 by construction and
    /// `pos` always lands on a character boundary — `advance` consumes whole codepoints. The
    /// boundary test guards the test-only byte constructor, which may hold malformed input;
    /// that path keeps the previous validating behaviour, including its empty-string fallback.
    pub fn remaining(&self) -> &'a str {
        match self.text {
            Some(text) if text.is_char_boundary(self.pos) => &text[self.pos..],
            _ => std::str::from_utf8(&self.bytes[self.pos..]).unwrap_or(""),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies `advance` makes forward progress on a malformed UTF-8 byte (yields the
    /// U+FFFD replacement char and consumes exactly one byte) so a scan loop can never
    /// spin on a non-advancing `None` — the latent hang class is removed at the source.
    #[test]
    fn advance_makes_progress_on_malformed_utf8() {
        let mut cursor = Cursor::from_bytes(&[0xff, b'a']);
        assert_eq!(cursor.advance(), Some('\u{FFFD}'));
        assert_eq!(cursor.advance(), Some('a'));
        assert_eq!(cursor.advance(), None);
        assert!(cursor.is_eof());
    }

    /// Verifies `peek` never returns `None` on a malformed byte while bytes remain, so a
    /// `while let Some(_) = peek()` loop also terminates via progress instead of hanging.
    #[test]
    fn peek_yields_replacement_on_malformed_utf8() {
        let cursor = Cursor::from_bytes(&[0xff]);
        assert_eq!(cursor.peek(), Some('\u{FFFD}'));
        assert!(!cursor.is_eof());
    }

    /// Verifies a valid multi-byte UTF-8 codepoint is still decoded whole and advances by
    /// its byte length, so the malformed-byte handling does not corrupt valid input.
    #[test]
    fn advance_decodes_valid_multibyte_codepoint() {
        let mut cursor = Cursor::from_bytes("é".as_bytes());
        assert_eq!(cursor.advance(), Some('é'));
        assert!(cursor.is_eof());
    }
}
