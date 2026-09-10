//! Purpose:
//! Encodes MIME headers with PHP's ASCII word folding and bounded encoded-word trials.
//!
//! Called from:
//! - The shared mbstring ABI after resolving language and charset defaults.
//!
//! Key details:
//! - Restarting the initial ASCII scan retains the decoder state from its last buffer.
//! - Trial chunks preserve destination call boundaries and include provisional final bytes.
//! - A single oversized character is permitted so encoding always makes progress.

use crate::encoding::Encoding;

const BUFFER: usize = 90;
const MIN_BUFFER: usize = 5;

/// Encodes a header using resolved source/destination codecs and PHP's line options.
pub fn encode_header(
    input: &[u8], source: Encoding, destination: Encoding, base64: bool,
    separator: &[u8], indent: i64,
) -> Vec<u8> {
    assert!(destination.supports_mime_header(), "validated MIME destination");
    if input.is_empty() { return Vec::new(); }
    let mut state = 0;
    if passthrough(input, source, &mut state) { return input.to_vec(); }
    let mut header = Header::new(destination, base64, separator, indent);
    let mut remaining = input;
    let mut buffer = Vec::new();
    while !remaining.is_empty() {
        let capacity = BUFFER - buffer.len();
        buffer.extend(source.decode_next(&mut remaining, capacity, &mut state).points);
        let (mut cursor, mut word_start) = (0, 0);
        while cursor < buffer.len() && buffer[cursor] == 32 && cursor <= 74 { cursor += 1; }
        while cursor < buffer.len() {
            let point = buffer[cursor];
            cursor += 1;
            if !(0x20..=0x7e).contains(&point) || matches!(point, 0x3f | 0x3d | 0x5f)
                || (point == 32 && cursor - word_start > 74)
            {
                header.before_encoded_word();
                return header.encoded_tail(buffer[word_start..].to_vec(), remaining, source, state);
            }
            if point == 32 {
                header.word_separator(cursor - word_start, 75);
                header.output.extend(buffer[word_start..cursor - 1].iter().map(|&point| point as u8));
                word_start = cursor;
                while cursor < buffer.len() && buffer[cursor] == 32 { cursor += 1; }
            }
        }
        if !remaining.is_empty() {
            if word_start < MIN_BUFFER {
                header.before_encoded_word();
                return header.encoded_tail(buffer[word_start..].to_vec(), remaining, source, state);
            }
            buffer.drain(..word_start);
        } else {
            if word_start < buffer.len() && !header.output.is_empty() {
                header.word_separator(cursor - word_start, 74);
            }
            header.output.extend(buffer[word_start..].iter().map(|&point| point as u8));
        }
    }
    header.output
}

/// Checks PHP's unbroken ASCII fast path while retaining the last complete decoder-call state.
fn passthrough(input: &[u8], source: Encoding, state: &mut u32) -> bool {
    let (mut remaining, mut leading_spaces) = (input, true);
    while !remaining.is_empty() {
        for point in source.decode_next(&mut remaining, BUFFER, state).points {
            if leading_spaces && point == 32 { continue; }
            leading_spaces = false;
            if !(0x21..=0x7e).contains(&point) || matches!(point, 0x3d | 0x3f | 0x5f) { return false; }
        }
    }
    true
}

/// Tracks emitted bytes separately from the indentation and the current line's starting offset.
struct Header<'a> {
    output: Vec<u8>,
    line_start: usize,
    indent: usize,
    separator: &'a [u8],
    destination: Encoding,
    mime_name: &'static str,
    base64: bool,
}

impl<'a> Header<'a> {
    /// Normalizes only the first-line indentation and PHP's truncated, NUL-terminated separator.
    fn new(destination: Encoding, base64: bool, separator: &'a [u8], indent: i64) -> Self {
        let separator = &separator[..separator.len().min(8)];
        let end = separator.iter().position(|&byte| byte == 0).unwrap_or(separator.len());
        Self { output: Vec::new(), line_start: 0, indent: if (0..74).contains(&indent) { indent as usize } else { 0 },
            separator: &separator[..end], destination, mime_name: destination.mime_name().unwrap(), base64 }
    }

    /// Starts a continuation line whose leading folding space is outside the next line's budget.
    fn fold(&mut self) {
        self.output.extend_from_slice(self.separator);
        self.output.push(b' ');
        self.indent = 0;
        self.line_start = self.output.len();
    }

    /// Separates an ASCII word using the caller's trailing-space or final-word limit.
    fn word_separator(&mut self, length: usize, limit: usize) {
        if self.output.len() - self.line_start + length + self.indent > limit { self.fold(); }
        else if !self.output.is_empty() { self.output.push(b' '); }
    }

    /// Leaves enough room for the charset prefix before switching permanently to encoded words.
    fn before_encoded_word(&mut self) {
        if self.output.len() - self.line_start + self.indent + self.mime_name.len() > 55 { self.fold(); }
        else if !self.output.is_empty() { self.output.push(b' '); }
    }

    /// Fits encoded words by replaying only each line's accepted bounded trial chunks.
    fn encoded_tail(mut self, mut buffer: Vec<u32>, mut input: &[u8], source: Encoding, mut state: u32) -> Vec<u8> {
        if BUFFER - buffer.len() >= MIN_BUFFER {
            buffer.extend(source.decode_next(&mut input, BUFFER - buffer.len(), &mut state).points);
        }
        let mut cursor = 0;
        loop {
            self.output.extend_from_slice(b"=?");
            self.output.extend_from_slice(self.mime_name.as_bytes());
            self.output.extend_from_slice(if self.base64 { b"?B?" } else { b"?Q?" });
            let available = 73 - self.indent - (self.output.len() - self.line_start);
            let (mut count, mut chunks) = (12, Vec::new());
            loop {
                assert!(cursor < buffer.len(), "MIME trial must have a source character");
                count = count.min(buffer.len() - cursor);
                let unflushed = self.destination.encode_mime_prefix(&chunks, false).len();
                chunks.push(buffer[cursor..cursor + count].to_vec());
                let trial = self.destination.encode_mime_prefix(&chunks, true);
                if transfer_size(&trial, self.base64) <= available || (count == 1 && unflushed == 0) {
                    cursor += count;
                    if cursor == buffer.len() {
                        // PHP returns at this boundary even when highly compact composite mappings
                        // let a full decoded buffer fit before the unread source has been exhausted.
                        transfer_bytes(&trial, self.base64, &mut self.output);
                        self.output.extend_from_slice(b"?=");
                        return self.output;
                    }
                } else {
                    chunks.pop();
                    if count > 1 { count = (count >> 1).max(1); continue; }
                    let bytes = self.destination.encode_mime_prefix(&chunks, true);
                    transfer_bytes(&bytes, self.base64, &mut self.output);
                    self.output.extend_from_slice(b"?=");
                    self.fold();
                    buffer.drain(..cursor);
                    cursor = 0;
                    if !input.is_empty() && BUFFER - buffer.len() >= MIN_BUFFER {
                        buffer.extend(source.decode_next(&mut input, BUFFER - buffer.len(), &mut state).points);
                    }
                    break;
                }
            }
        }
    }
}

/// Identifies the historical MIME Q safe set, which excludes ASCII digits and spaces.
fn q_safe(byte: u8) -> bool { byte.is_ascii_alphabetic() || b"!*+-/".contains(&byte) }

/// Measures the transfer representation including Base64 padding or complete Q escapes.
fn transfer_size(input: &[u8], base64: bool) -> usize {
    if base64 { input.len().div_ceil(3) * 4 }
    else { input.iter().map(|&byte| if q_safe(byte) { 1 } else { 3 }).sum() }
}

/// Writes one payload without inserting transfer-specific lines inside its MIME word.
fn transfer_bytes(input: &[u8], base64: bool, output: &mut Vec<u8>) {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    if base64 {
        for bytes in input.chunks(3) {
            let (a, b, c) = (bytes[0], bytes.get(1).copied().unwrap_or(0), bytes.get(2).copied().unwrap_or(0));
            output.extend_from_slice(&[ALPHABET[usize::from(a >> 2)], ALPHABET[usize::from(((a & 3) << 4) | (b >> 4))],
                if bytes.len() > 1 { ALPHABET[usize::from(((b & 15) << 2) | (c >> 6))] } else { b'=' },
                if bytes.len() > 2 { ALPHABET[usize::from(c & 63)] } else { b'=' }]);
        }
    } else {
        for &byte in input {
            if q_safe(byte) { output.push(byte); }
            else { output.extend_from_slice(&[b'=', HEX[usize::from(byte >> 4)], HEX[usize::from(byte & 15)]]); }
        }
    }
}
