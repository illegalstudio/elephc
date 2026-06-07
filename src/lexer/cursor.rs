//! Purpose:
//! Provides byte-level source traversal while tracking line and column spans.
//! Offers small peek/advance helpers used by lexer scanners.
//!
//! Called from:
//! - `crate::lexer::scan` and `crate::lexer::literals` scanners.
//!
//! Key details:
//! - Span positions are one-based and advance through newlines so diagnostics map back to PHP source.

use crate::span::Span;

/// Lexer cursor for source tracking.
pub struct Cursor<'a> {
    bytes: &'a [u8],
    pos: usize,
    line: usize,
    col: usize,
}

impl<'a> Cursor<'a> {
    /// Constructs a new cursor over the given source string.
    ///
    /// The cursor starts at position 0, line 1, column 1 (one-based spans).
    pub fn new(source: &'a str) -> Self {
        Self {
            bytes: source.as_bytes(),
            pos: 0,
            line: 1,
            col: 1,
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
            pos: 0,
            line: 1,
            col: 1,
        }
    }

    /// Returns the current source position as a one-based `Span` (line, column).
    ///
    /// Used to attach source locations to tokens for diagnostics.
    pub fn span(&self) -> Span {
        Span::new(self.line, self.col)
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
            std::str::from_utf8(&self.bytes[self.pos..])
                .ok()
                .and_then(|s| s.chars().next())
                .or(Some('\u{FFFD}'))
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
            match std::str::from_utf8(&self.bytes[self.pos..])
                .ok()
                .and_then(|s| s.chars().next())
            {
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

    /// Returns true if the cursor has reached the end of source.
    pub fn is_eof(&self) -> bool {
        self.pos >= self.bytes.len()
    }

    /// Returns the slice of source from the current position to the end, as a `&str`.
    ///
    /// The slice is guaranteed to be valid UTF-8 (panics if byte range is malformed).
    pub fn remaining(&self) -> &'a str {
        std::str::from_utf8(&self.bytes[self.pos..]).unwrap_or("")
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
