//! Purpose:
//! Validates JSON byte streams for eval-side `json_validate()` calls.
//! The validator checks syntax and depth without allocating decoded PHP values.
//!
//! Called from:
//! - `crate::interpreter` when dispatching eval `json_validate()`.
//!
//! Key details:
//! - Container depth follows PHP decode/validate semantics: entering a container
//!   is rejected when the active depth would reach the requested limit.
//! - String validation accepts JSON escapes, paired UTF-16 surrogate escapes,
//!   and raw UTF-8 bytes while rejecting control bytes and malformed UTF-8.

/// Returns whether one byte slice is a complete JSON document within the depth limit.
pub(crate) fn bytes(bytes: &[u8], depth_limit: usize) -> bool {
    let mut parser = Validator::new(bytes, depth_limit);
    parser.parse_document()
}

/// Cursor-based JSON validator for eval `json_validate()` calls.
struct Validator<'a> {
    bytes: &'a [u8],
    cursor: usize,
    depth_limit: usize,
}

impl<'a> Validator<'a> {
    /// Creates a JSON validator over one immutable byte slice.
    fn new(bytes: &'a [u8], depth_limit: usize) -> Self {
        Self {
            bytes,
            cursor: 0,
            depth_limit,
        }
    }

    /// Parses one complete JSON document and rejects trailing non-whitespace bytes.
    fn parse_document(&mut self) -> bool {
        self.skip_ws();
        if !self.parse_value(0) {
            return false;
        }
        self.skip_ws();
        self.cursor == self.bytes.len()
    }

    /// Parses any JSON value at the given active container depth.
    fn parse_value(&mut self, depth: usize) -> bool {
        self.skip_ws();
        match self.peek() {
            Some(b'n') => self.consume_literal(b"null"),
            Some(b't') => self.consume_literal(b"true"),
            Some(b'f') => self.consume_literal(b"false"),
            Some(b'"') => self.parse_string(),
            Some(b'[') => self.parse_array(depth),
            Some(b'{') => self.parse_object(depth),
            Some(b'-' | b'0'..=b'9') => self.parse_number(),
            _ => false,
        }
    }

    /// Parses a JSON array and enforces PHP's validate/decode depth threshold.
    fn parse_array(&mut self, depth: usize) -> bool {
        if depth + 1 >= self.depth_limit {
            return false;
        }
        self.cursor += 1;
        self.skip_ws();
        if self.consume_byte(b']') {
            return true;
        }

        loop {
            if !self.parse_value(depth + 1) {
                return false;
            }
            self.skip_ws();
            if self.consume_byte(b']') {
                return true;
            }
            if !self.consume_byte(b',') {
                return false;
            }
        }
    }

    /// Parses a JSON object and enforces PHP's validate/decode depth threshold.
    fn parse_object(&mut self, depth: usize) -> bool {
        if depth + 1 >= self.depth_limit {
            return false;
        }
        self.cursor += 1;
        self.skip_ws();
        if self.consume_byte(b'}') {
            return true;
        }

        loop {
            self.skip_ws();
            if !self.parse_string() {
                return false;
            }
            self.skip_ws();
            if !self.consume_byte(b':') {
                return false;
            }
            if !self.parse_value(depth + 1) {
                return false;
            }
            self.skip_ws();
            if self.consume_byte(b'}') {
                return true;
            }
            if !self.consume_byte(b',') {
                return false;
            }
        }
    }

    /// Parses a JSON string, including escapes, UTF-8 bytes, and surrogate-pair escapes.
    fn parse_string(&mut self) -> bool {
        if !self.consume_byte(b'"') {
            return false;
        }

        while let Some(byte) = self.peek() {
            match byte {
                b'"' => {
                    self.cursor += 1;
                    return true;
                }
                b'\\' => {
                    if !self.parse_string_escape() {
                        return false;
                    }
                }
                0x00..=0x1f => return false,
                0x00..=0x7f => self.cursor += 1,
                _ => {
                    if !self.consume_utf8_char() {
                        return false;
                    }
                }
            }
        }
        false
    }

    /// Parses one JSON string escape sequence at the current backslash.
    fn parse_string_escape(&mut self) -> bool {
        self.cursor += 1;
        match self.peek() {
            Some(b'"' | b'\\' | b'/' | b'b' | b'f' | b'n' | b'r' | b't') => {
                self.cursor += 1;
                true
            }
            Some(b'u') => self.parse_unicode_escape(),
            _ => false,
        }
    }

    /// Parses one JSON `\uXXXX` escape, including mandatory surrogate pairs.
    fn parse_unicode_escape(&mut self) -> bool {
        let Some(unit) = self.parse_unicode_unit() else {
            return false;
        };
        if (0xd800..=0xdbff).contains(&unit) {
            let checkpoint = self.cursor;
            if !self.consume_byte(b'\\') || !self.consume_byte(b'u') {
                return false;
            }
            let Some(low) = self.parse_unicode_unit_after_u() else {
                self.cursor = checkpoint;
                return false;
            };
            (0xdc00..=0xdfff).contains(&low)
        } else {
            !(0xdc00..=0xdfff).contains(&unit)
        }
    }

    /// Parses the `uXXXX` suffix after the backslash has already been consumed.
    fn parse_unicode_unit(&mut self) -> Option<u16> {
        if !self.consume_byte(b'u') {
            return None;
        }
        self.parse_unicode_unit_after_u()
    }

    /// Parses the four hex digits after a consumed JSON unicode escape marker.
    fn parse_unicode_unit_after_u(&mut self) -> Option<u16> {
        if self.cursor + 4 > self.bytes.len() {
            return None;
        }
        let mut value = 0_u16;
        for _ in 0..4 {
            value = value.checked_mul(16)?;
            value = value.checked_add(u16::from(hex_value(self.bytes[self.cursor])?))?;
            self.cursor += 1;
        }
        Some(value)
    }

    /// Parses a JSON number with RFC-compatible leading-zero, fraction, and exponent rules.
    fn parse_number(&mut self) -> bool {
        if self.consume_byte(b'-') && self.peek().is_none() {
            return false;
        }

        match self.peek() {
            Some(b'0') => {
                self.cursor += 1;
                if matches!(self.peek(), Some(b'0'..=b'9')) {
                    return false;
                }
            }
            Some(b'1'..=b'9') => {
                self.cursor += 1;
                while matches!(self.peek(), Some(b'0'..=b'9')) {
                    self.cursor += 1;
                }
            }
            _ => return false,
        }

        if self.consume_byte(b'.') {
            if !matches!(self.peek(), Some(b'0'..=b'9')) {
                return false;
            }
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.cursor += 1;
            }
        }

        if matches!(self.peek(), Some(b'e' | b'E')) {
            self.cursor += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.cursor += 1;
            }
            if !matches!(self.peek(), Some(b'0'..=b'9')) {
                return false;
            }
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.cursor += 1;
            }
        }

        true
    }

    /// Consumes exactly one expected byte when it is present.
    fn consume_byte(&mut self, expected: u8) -> bool {
        if self.peek() == Some(expected) {
            self.cursor += 1;
            true
        } else {
            false
        }
    }

    /// Consumes one ASCII literal at the current cursor.
    fn consume_literal(&mut self, literal: &[u8]) -> bool {
        if self.bytes[self.cursor..].starts_with(literal) {
            self.cursor += literal.len();
            true
        } else {
            false
        }
    }

    /// Consumes one valid UTF-8 codepoint from a raw JSON string segment.
    fn consume_utf8_char(&mut self) -> bool {
        let first = self.bytes[self.cursor];
        let width = match first {
            0xc2..=0xdf => 2,
            0xe0..=0xef => 3,
            0xf0..=0xf4 => 4,
            _ => return false,
        };
        if self.cursor + width > self.bytes.len() {
            return false;
        }
        let slice = &self.bytes[self.cursor..self.cursor + width];
        if std::str::from_utf8(slice).is_err() {
            return false;
        }
        self.cursor += width;
        true
    }

    /// Skips JSON whitespace accepted between tokens.
    fn skip_ws(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\n' | b'\r' | b'\t')) {
            self.cursor += 1;
        }
    }

    /// Returns the current byte without advancing.
    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.cursor).copied()
    }
}

/// Returns one hexadecimal digit value for JSON unicode escapes.
fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}
