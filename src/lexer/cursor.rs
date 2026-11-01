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
    /// The same source as `&str`, retained so `remaining()` can slice it in O(1).
    ///
    /// `None` only for the test-only byte constructor, whose input may be malformed. See
    /// `remaining` for why holding this matters.
    text: Option<&'a str>,
    pos: usize,
    line: usize,
    col: usize,
    source_id: u32,
}

impl<'a> Cursor<'a> {
    /// Constructs a cursor starting on `line` whose spans identify one physical source file.
    ///
    /// Bytes removed before scanning must not renumber what follows them: PHP's CLI drops a
    /// leading `#!` line but still reports a `throw` on physical line 3 as line 3. The scan gets
    /// the remainder and the line it starts on, so the two stay consistent.
    pub(super) fn new_at_line_in_source(source: &'a str, line: usize, source_id: u32) -> Self {
        Self {
            bytes: source.as_bytes(),
            text: Some(source),
            pos: 0,
            line,
            col: 1,
            source_id,
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
            self.decode_char().or(Some('\u{FFFD}'))
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
            match self.decode_char() {
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

    /// Decodes the codepoint starting at `pos`, or `None` on a malformed byte.
    ///
    /// Looks at the next four bytes at most: validating the whole tail per non-ASCII character
    /// made lexing quadratic in the distance to the end of the file (see [`Self::remaining`]).
    fn decode_char(&self) -> Option<char> {
        let window = &self.bytes[self.pos..self.bytes.len().min(self.pos + 4)];
        let valid = match std::str::from_utf8(window) {
            Ok(text) => text,
            // The window may cut the character after this one; its valid prefix still holds
            // this one whole.
            Err(error) => std::str::from_utf8(&window[..error.valid_up_to()]).ok()?,
        };
        valid.chars().next()
    }

    /// Returns true if the cursor has reached the end of source.
    pub fn is_eof(&self) -> bool {
        self.pos >= self.bytes.len()
    }

    /// Returns the slice of source from the current position to the end, as a `&str`.
    ///
    /// THIS MUST STAY O(1). `scan` calls it several times per token — to test for `<?php`,
    /// `//`, `/*`, `#[`, `?->`, `??`, a heredoc opener and so on — so any cost proportional to
    /// the REMAINING source makes tokenizing quadratic in file size. It used to re-run
    /// `str::from_utf8` over the whole tail on every call, which validates every byte: a 555 KB
    /// file spent 7.6 s in the tokenizer, and doubling the input quadrupled the time. Slicing a
    /// `&str` that was validated once instead makes it a pointer adjustment.
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

    /// Decoding reads only the character at the cursor: a valid codepoint followed by a
    /// malformed tail still decodes, and adjacent multi-byte codepoints whose four-byte window
    /// cuts the next one decode whole. Validating the whole remaining tail per character made
    /// lexing quadratic, and it also turned every valid character before a bad byte into U+FFFD.
    #[test]
    fn decoding_looks_only_at_the_current_character() {
        let mut bytes = "é😀€".as_bytes().to_vec();
        bytes.push(0xff);
        let mut cursor = Cursor::from_bytes(&bytes);
        assert_eq!(cursor.peek(), Some('é'));
        assert_eq!(cursor.advance(), Some('é'));
        assert_eq!(cursor.advance(), Some('😀'));
        assert_eq!(cursor.advance(), Some('€'));
        assert_eq!(cursor.advance(), Some('\u{FFFD}'));
        assert!(cursor.is_eof());
    }
}
