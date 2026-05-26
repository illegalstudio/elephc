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
            // Fallback for non-ASCII (rare in PHP source)
            std::str::from_utf8(&self.bytes[self.pos..])
                .ok()?
                .chars()
                .next()
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
            let s = std::str::from_utf8(&self.bytes[self.pos..]).ok()?;
            let ch = s.chars().next()?;
            self.pos += ch.len_utf8();
            ch
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
