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
//!   UTF-8 bytes while rejecting control bytes; malformed UTF-8 is rejected by
//!   default and can be ignored for PHP's `JSON_INVALID_UTF8_IGNORE` validate flag.

/// Parsed JSON value used by eval JSON builtins before runtime-cell allocation.
pub(crate) enum JsonValue {
    Null,
    Bool(bool),
    Number(Vec<u8>),
    String(Vec<u8>),
    Array(Vec<JsonValue>),
    Object(Vec<(Vec<u8>, JsonValue)>),
}

/// PHP JSON error produced while parsing eval-side JSON bytes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct JsonParseError {
    kind: JsonParseErrorKind,
    offset: usize,
}

impl JsonParseError {
    /// Creates one parser error at a zero-based byte offset.
    const fn new(kind: JsonParseErrorKind, offset: usize) -> Self {
        Self { kind, offset }
    }

    /// Returns the PHP JSON error category for this parse failure.
    pub(crate) const fn kind(self) -> JsonParseErrorKind {
        self.kind
    }

    /// Returns the zero-based byte offset where parsing failed.
    pub(crate) const fn offset(self) -> usize {
        self.offset
    }
}

/// PHP JSON error category produced while parsing eval-side JSON bytes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum JsonParseErrorKind {
    Depth,
    Syntax,
    ControlChar,
    Utf8,
    Utf16,
}

/// Parses one complete JSON document and preserves the PHP-visible error category.
pub(crate) fn decode_result(
    bytes: &[u8],
    depth_limit: usize,
) -> Result<JsonValue, JsonParseError> {
    let mut parser = Parser::new(bytes, depth_limit);
    parser.parse_document()
}

/// Parses one complete JSON document while ignoring malformed raw UTF-8 string bytes.
pub(crate) fn decode_result_ignoring_invalid_utf8(
    bytes: &[u8],
    depth_limit: usize,
) -> Result<JsonValue, JsonParseError> {
    let mut parser = Parser::new_with_invalid_utf8_ignore(bytes, depth_limit);
    parser.parse_document()
}

/// Cursor-based JSON parser for eval JSON builtin calls.
struct Parser<'a> {
    bytes: &'a [u8],
    cursor: usize,
    depth_limit: usize,
    ignore_invalid_utf8: bool,
}

impl<'a> Parser<'a> {
    /// Creates a JSON parser over one immutable byte slice.
    fn new(bytes: &'a [u8], depth_limit: usize) -> Self {
        Self {
            bytes,
            cursor: 0,
            depth_limit,
            ignore_invalid_utf8: false,
        }
    }

    /// Creates a JSON parser that drops malformed raw UTF-8 bytes inside strings.
    fn new_with_invalid_utf8_ignore(bytes: &'a [u8], depth_limit: usize) -> Self {
        Self {
            bytes,
            cursor: 0,
            depth_limit,
            ignore_invalid_utf8: true,
        }
    }

    /// Parses one complete JSON document and rejects trailing non-whitespace bytes.
    fn parse_document(&mut self) -> Result<JsonValue, JsonParseError> {
        self.skip_ws();
        let value = self.parse_value(0)?;
        self.skip_ws();
        if self.cursor == self.bytes.len() {
            Ok(value)
        } else {
            Err(self.error(JsonParseErrorKind::Syntax))
        }
    }

    /// Parses any JSON value at the given active container depth.
    fn parse_value(&mut self, depth: usize) -> Result<JsonValue, JsonParseError> {
        self.skip_ws();
        match self.peek().ok_or_else(|| self.error(JsonParseErrorKind::Syntax))? {
            b'n' => self.consume_literal_value(b"null", JsonValue::Null),
            b't' => self.consume_literal_value(b"true", JsonValue::Bool(true)),
            b'f' => self.consume_literal_value(b"false", JsonValue::Bool(false)),
            b'"' => self.parse_string().map(JsonValue::String),
            b'[' => self.parse_array(depth),
            b'{' => self.parse_object(depth),
            b'-' | b'0'..=b'9' => self.parse_number().map(JsonValue::Number),
            _ => Err(self.error(JsonParseErrorKind::Syntax)),
        }
    }

    /// Consumes one JSON literal and returns its parsed value.
    fn consume_literal_value(
        &mut self,
        literal: &[u8],
        value: JsonValue,
    ) -> Result<JsonValue, JsonParseError> {
        if self.consume_literal(literal) {
            Ok(value)
        } else {
            Err(self.error(JsonParseErrorKind::Syntax))
        }
    }

    /// Parses a JSON array and enforces PHP's validate/decode depth threshold.
    fn parse_array(&mut self, depth: usize) -> Result<JsonValue, JsonParseError> {
        if depth + 1 >= self.depth_limit {
            return Err(self.error(JsonParseErrorKind::Depth));
        }
        self.cursor += 1;
        self.skip_ws();
        let mut elements = Vec::new();
        if self.consume_byte(b']') {
            return Ok(JsonValue::Array(elements));
        }

        loop {
            elements.push(self.parse_value(depth + 1)?);
            self.skip_ws();
            if self.consume_byte(b']') {
                return Ok(JsonValue::Array(elements));
            }
            if !self.consume_byte(b',') {
                return Err(self.error(JsonParseErrorKind::Syntax));
            }
        }
    }

    /// Parses a JSON object and enforces PHP's validate/decode depth threshold.
    fn parse_object(&mut self, depth: usize) -> Result<JsonValue, JsonParseError> {
        if depth + 1 >= self.depth_limit {
            return Err(self.error(JsonParseErrorKind::Depth));
        }
        self.cursor += 1;
        self.skip_ws();
        let mut entries = Vec::new();
        if self.consume_byte(b'}') {
            return Ok(JsonValue::Object(entries));
        }

        loop {
            self.skip_ws();
            let key = self.parse_string()?;
            self.skip_ws();
            if !self.consume_byte(b':') {
                return Err(self.error(JsonParseErrorKind::Syntax));
            }
            entries.push((key, self.parse_value(depth + 1)?));
            self.skip_ws();
            if self.consume_byte(b'}') {
                return Ok(JsonValue::Object(entries));
            }
            if !self.consume_byte(b',') {
                return Err(self.error(JsonParseErrorKind::Syntax));
            }
        }
    }

    /// Parses a JSON string into UTF-8 bytes after applying JSON escapes.
    fn parse_string(&mut self) -> Result<Vec<u8>, JsonParseError> {
        if !self.consume_byte(b'"') {
            return Err(self.error(JsonParseErrorKind::Syntax));
        }

        let mut output = Vec::new();
        while let Some(byte) = self.peek() {
            match byte {
                b'"' => {
                    self.cursor += 1;
                    return Ok(output);
                }
                b'\\' => {
                    self.parse_string_escape(&mut output)?;
                }
                0x00..=0x1f => return Err(self.error(JsonParseErrorKind::ControlChar)),
                0x00..=0x7f => {
                    output.push(byte);
                    self.cursor += 1;
                }
                _ => {
                    let start = self.cursor;
                    match self.consume_utf8_char() {
                        Ok(()) => output.extend_from_slice(&self.bytes[start..self.cursor]),
                        Err(_) if self.ignore_invalid_utf8 => {
                            self.cursor = start + 1;
                        }
                        Err(error) => return Err(error),
                    }
                }
            }
        }
        Err(self.error(JsonParseErrorKind::Syntax))
    }

    /// Parses one JSON string escape sequence at the current backslash.
    fn parse_string_escape(&mut self, output: &mut Vec<u8>) -> Result<(), JsonParseError> {
        self.cursor += 1;
        match self.peek().ok_or_else(|| self.error(JsonParseErrorKind::Syntax))? {
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
                return Ok(());
            }
            _ => return Err(self.error(JsonParseErrorKind::Syntax)),
        }
        self.cursor += 1;
        Ok(())
    }

    /// Parses one JSON `\uXXXX` escape, including mandatory surrogate pairs.
    fn parse_unicode_escape(&mut self, output: &mut Vec<u8>) -> Result<(), JsonParseError> {
        let unit = self.parse_unicode_unit()?;
        if (0xd800..=0xdbff).contains(&unit) {
            if !self.consume_byte(b'\\') || !self.consume_byte(b'u') {
                return Err(self.error(JsonParseErrorKind::Utf16));
            }
            let low = match self.parse_unicode_unit_after_u() {
                Ok(low) => low,
                Err(_) => return Err(self.error(JsonParseErrorKind::Utf16)),
            };
            if !(0xdc00..=0xdfff).contains(&low) {
                return Err(self.error(JsonParseErrorKind::Utf16));
            }
            let high = u32::from(unit - 0xd800);
            let low = u32::from(low - 0xdc00);
            append_codepoint(output, 0x10000 + ((high << 10) | low))
                .ok_or_else(|| self.error(JsonParseErrorKind::Utf16))
        } else if (0xdc00..=0xdfff).contains(&unit) {
            Err(self.error(JsonParseErrorKind::Utf16))
        } else {
            append_codepoint(output, u32::from(unit))
                .ok_or_else(|| self.error(JsonParseErrorKind::Utf16))
        }
    }

    /// Parses the `uXXXX` suffix after the backslash has already been consumed.
    fn parse_unicode_unit(&mut self) -> Result<u16, JsonParseError> {
        if !self.consume_byte(b'u') {
            return Err(self.error(JsonParseErrorKind::Syntax));
        }
        self.parse_unicode_unit_after_u()
    }

    /// Parses the four hex digits after a consumed JSON unicode escape marker.
    fn parse_unicode_unit_after_u(&mut self) -> Result<u16, JsonParseError> {
        if self.cursor + 4 > self.bytes.len() {
            return Err(self.error(JsonParseErrorKind::Syntax));
        }
        let mut value = 0_u16;
        for _ in 0..4 {
            let digit = hex_value(self.bytes[self.cursor])
                .ok_or_else(|| self.error(JsonParseErrorKind::Syntax))?;
            value = value * 16 + u16::from(digit);
            self.cursor += 1;
        }
        Ok(value)
    }

    /// Parses a JSON number with RFC-compatible leading-zero, fraction, and exponent rules.
    fn parse_number(&mut self) -> Result<Vec<u8>, JsonParseError> {
        let start = self.cursor;
        if self.consume_byte(b'-') && self.peek().is_none() {
            return Err(self.error(JsonParseErrorKind::Syntax));
        }

        match self.peek().ok_or_else(|| self.error(JsonParseErrorKind::Syntax))? {
            b'0' => {
                self.cursor += 1;
                if matches!(self.peek(), Some(b'0'..=b'9')) {
                    return Err(self.error(JsonParseErrorKind::Syntax));
                }
            }
            b'1'..=b'9' => {
                self.cursor += 1;
                while matches!(self.peek(), Some(b'0'..=b'9')) {
                    self.cursor += 1;
                }
            }
            _ => return Err(self.error(JsonParseErrorKind::Syntax)),
        }

        if self.consume_byte(b'.') {
            if !matches!(self.peek(), Some(b'0'..=b'9')) {
                return Err(self.error(JsonParseErrorKind::Syntax));
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
                return Err(self.error(JsonParseErrorKind::Syntax));
            }
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.cursor += 1;
            }
        }

        Ok(self.bytes[start..self.cursor].to_vec())
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
    fn consume_utf8_char(&mut self) -> Result<(), JsonParseError> {
        let first = self.bytes[self.cursor];
        let width = match first {
            0xc2..=0xdf => 2,
            0xe0..=0xef => 3,
            0xf0..=0xf4 => 4,
            _ => return Err(self.error(JsonParseErrorKind::Utf8)),
        };
        if self.cursor + width > self.bytes.len() {
            return Err(self.error(JsonParseErrorKind::Utf8));
        }
        let slice = &self.bytes[self.cursor..self.cursor + width];
        if std::str::from_utf8(slice).is_err() {
            return Err(self.error(JsonParseErrorKind::Utf8));
        }
        self.cursor += width;
        Ok(())
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

    /// Creates a parser error at the current cursor byte offset.
    fn error(&self, kind: JsonParseErrorKind) -> JsonParseError {
        JsonParseError::new(kind, self.cursor)
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
