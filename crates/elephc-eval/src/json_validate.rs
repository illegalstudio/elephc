//! Purpose:
//! Parses JSON byte streams for eval-side `json_validate()` and `json_decode()`.
//! The parser checks syntax and can return a small JSON tree for runtime-cell materialization.
//!
//! Called from:
//! - `crate::interpreter` when dispatching eval JSON builtins.
//!
//! Key details:
//! - Container depth follows PHP decode/validate semantics: entering a container
//!   is rejected when the active depth would reach the requested limit.
//! - String parsing accepts JSON escapes, paired UTF-16 surrogate escapes, and raw
//!   UTF-8 bytes while rejecting control bytes and malformed UTF-8.

/// Parsed JSON value used by eval JSON builtins before runtime-cell allocation.
pub(crate) enum JsonValue {
    Null,
    Bool(bool),
    Number(Vec<u8>),
    String(Vec<u8>),
    Array(Vec<JsonValue>),
    Object(Vec<(Vec<u8>, JsonValue)>),
}

/// Returns whether one byte slice is a complete JSON document within the depth limit.
pub(crate) fn bytes(bytes: &[u8], depth_limit: usize) -> bool {
    decode(bytes, depth_limit).is_some()
}

/// Parses one complete JSON document into an eval-side JSON value.
pub(crate) fn decode(bytes: &[u8], depth_limit: usize) -> Option<JsonValue> {
    let mut parser = Parser::new(bytes, depth_limit);
    parser.parse_document()
}

/// Cursor-based JSON parser for eval JSON builtin calls.
struct Parser<'a> {
    bytes: &'a [u8],
    cursor: usize,
    depth_limit: usize,
}

impl<'a> Parser<'a> {
    /// Creates a JSON parser over one immutable byte slice.
    fn new(bytes: &'a [u8], depth_limit: usize) -> Self {
        Self {
            bytes,
            cursor: 0,
            depth_limit,
        }
    }

    /// Parses one complete JSON document and rejects trailing non-whitespace bytes.
    fn parse_document(&mut self) -> Option<JsonValue> {
        self.skip_ws();
        let value = self.parse_value(0)?;
        self.skip_ws();
        (self.cursor == self.bytes.len()).then_some(value)
    }

    /// Parses any JSON value at the given active container depth.
    fn parse_value(&mut self, depth: usize) -> Option<JsonValue> {
        self.skip_ws();
        match self.peek()? {
            b'n' => self.consume_literal(b"null").then_some(JsonValue::Null),
            b't' => self.consume_literal(b"true").then_some(JsonValue::Bool(true)),
            b'f' => self.consume_literal(b"false").then_some(JsonValue::Bool(false)),
            b'"' => self.parse_string().map(JsonValue::String),
            b'[' => self.parse_array(depth),
            b'{' => self.parse_object(depth),
            b'-' | b'0'..=b'9' => self.parse_number().map(JsonValue::Number),
            _ => None,
        }
    }

    /// Parses a JSON array and enforces PHP's validate/decode depth threshold.
    fn parse_array(&mut self, depth: usize) -> Option<JsonValue> {
        if depth + 1 >= self.depth_limit {
            return None;
        }
        self.cursor += 1;
        self.skip_ws();
        let mut elements = Vec::new();
        if self.consume_byte(b']') {
            return Some(JsonValue::Array(elements));
        }

        loop {
            elements.push(self.parse_value(depth + 1)?);
            self.skip_ws();
            if self.consume_byte(b']') {
                return Some(JsonValue::Array(elements));
            }
            if !self.consume_byte(b',') {
                return None;
            }
        }
    }

    /// Parses a JSON object and enforces PHP's validate/decode depth threshold.
    fn parse_object(&mut self, depth: usize) -> Option<JsonValue> {
        if depth + 1 >= self.depth_limit {
            return None;
        }
        self.cursor += 1;
        self.skip_ws();
        let mut entries = Vec::new();
        if self.consume_byte(b'}') {
            return Some(JsonValue::Object(entries));
        }

        loop {
            self.skip_ws();
            let key = self.parse_string()?;
            self.skip_ws();
            if !self.consume_byte(b':') {
                return None;
            }
            entries.push((key, self.parse_value(depth + 1)?));
            self.skip_ws();
            if self.consume_byte(b'}') {
                return Some(JsonValue::Object(entries));
            }
            if !self.consume_byte(b',') {
                return None;
            }
        }
    }

    /// Parses a JSON string into UTF-8 bytes after applying JSON escapes.
    fn parse_string(&mut self) -> Option<Vec<u8>> {
        if !self.consume_byte(b'"') {
            return None;
        }

        let mut output = Vec::new();
        while let Some(byte) = self.peek() {
            match byte {
                b'"' => {
                    self.cursor += 1;
                    return Some(output);
                }
                b'\\' => {
                    self.parse_string_escape(&mut output)?;
                }
                0x00..=0x1f => return None,
                0x00..=0x7f => {
                    output.push(byte);
                    self.cursor += 1;
                }
                _ => {
                    let start = self.cursor;
                    self.consume_utf8_char()?;
                    output.extend_from_slice(&self.bytes[start..self.cursor]);
                }
            }
        }
        None
    }

    /// Parses one JSON string escape sequence at the current backslash.
    fn parse_string_escape(&mut self, output: &mut Vec<u8>) -> Option<()> {
        self.cursor += 1;
        match self.peek()? {
            b'"' => output.push(b'"'),
            b'\\' => output.push(b'\\'),
            b'/' => output.push(b'/'),
            b'b' => output.push(0x08),
            b'f' => output.push(0x0c),
            b'n' => output.push(b'\n'),
            b'r' => output.push(b'\r'),
            b't' => output.push(b'\t'),
            b'u' => {
                self.parse_unicode_escape(output)?;
                return Some(());
            }
            _ => return None,
        }
        self.cursor += 1;
        Some(())
    }

    /// Parses one JSON `\uXXXX` escape, including mandatory surrogate pairs.
    fn parse_unicode_escape(&mut self, output: &mut Vec<u8>) -> Option<()> {
        let unit = self.parse_unicode_unit()?;
        if (0xd800..=0xdbff).contains(&unit) {
            let checkpoint = self.cursor;
            if !self.consume_byte(b'\\') || !self.consume_byte(b'u') {
                return None;
            }
            let Some(low) = self.parse_unicode_unit_after_u() else {
                self.cursor = checkpoint;
                return None;
            };
            if !(0xdc00..=0xdfff).contains(&low) {
                return None;
            }
            let high = u32::from(unit - 0xd800);
            let low = u32::from(low - 0xdc00);
            append_codepoint(output, 0x10000 + ((high << 10) | low))
        } else if (0xdc00..=0xdfff).contains(&unit) {
            None
        } else {
            append_codepoint(output, u32::from(unit))
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
    fn parse_number(&mut self) -> Option<Vec<u8>> {
        let start = self.cursor;
        if self.consume_byte(b'-') && self.peek().is_none() {
            return None;
        }

        match self.peek()? {
            b'0' => {
                self.cursor += 1;
                if matches!(self.peek(), Some(b'0'..=b'9')) {
                    return None;
                }
            }
            b'1'..=b'9' => {
                self.cursor += 1;
                while matches!(self.peek(), Some(b'0'..=b'9')) {
                    self.cursor += 1;
                }
            }
            _ => return None,
        }

        if self.consume_byte(b'.') {
            if !matches!(self.peek(), Some(b'0'..=b'9')) {
                return None;
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
                return None;
            }
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.cursor += 1;
            }
        }

        Some(self.bytes[start..self.cursor].to_vec())
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
    fn consume_utf8_char(&mut self) -> Option<()> {
        let first = self.bytes[self.cursor];
        let width = match first {
            0xc2..=0xdf => 2,
            0xe0..=0xef => 3,
            0xf0..=0xf4 => 4,
            _ => return None,
        };
        if self.cursor + width > self.bytes.len() {
            return None;
        }
        let slice = &self.bytes[self.cursor..self.cursor + width];
        if std::str::from_utf8(slice).is_err() {
            return None;
        }
        self.cursor += width;
        Some(())
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

/// Appends one Unicode codepoint to a decoded JSON string.
fn append_codepoint(output: &mut Vec<u8>, codepoint: u32) -> Option<()> {
    let ch = char::from_u32(codepoint)?;
    let mut buffer = [0_u8; 4];
    output.extend_from_slice(ch.encode_utf8(&mut buffer).as_bytes());
    Some(())
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
