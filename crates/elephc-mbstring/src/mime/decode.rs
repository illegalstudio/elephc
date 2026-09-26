//! Purpose:
//! Decodes MIME encoded words and folds header whitespace with PHP's permissive rules.
//!
//! Called from:
//! - `super::decode_header` through the shared mbstring bridge.
//!
//! Key details:
//! - Invalid words remain ASCII text; accepted words may omit their final terminator.
//! - Decoder calls and output chunk boundaries remain distinct between encoded words.

use crate::encoding::Encoding;

/// Converts a complete MIME header into the current internal encoding.
pub fn decode_header(input: &[u8], destination: Encoding) -> Vec<u8> {
    let ascii = Encoding::lookup(b"ASCII").expect("registered ASCII codec");
    let mut chunks = Vec::new();
    let mut decoder_state = 0;
    let (mut offset, mut space_pending) = (0, false);
    while offset < input.len() {
        if let Some((encoding, bytes, end)) = encoded_word(&input[offset..]) {
            append_decoded(&mut chunks, encoding, &bytes, &mut decoder_state);
            offset += end;
            space_pending = input.get(offset).is_some_and(|&byte| whitespace(byte));
            while input.get(offset).is_some_and(|&byte| whitespace(byte)) { offset += 1; }
            continue;
        }
        if space_pending {
            chunks.push(vec![u32::from(b' ')]);
            space_pending = false;
        }
        if !matches!(input[offset], b'\r' | b'\n') {
            let mut end = offset + 1;
            while end < input.len() && !matches!(input[end], b'=' | b'\r' | b'\n') { end += 1; }
            append_decoded(&mut chunks, ascii, &input[offset..end], &mut decoder_state);
            offset = end;
        }
        if input.get(offset).is_some_and(|byte| matches!(byte, b'\r' | b'\n')) {
            while input.get(offset).is_some_and(|&byte| whitespace(byte)) { offset += 1; }
            if offset < input.len() { chunks.push(vec![u32::from(b' ')]); }
        }
    }
    destination.encode_mime_chunks(&chunks)
}

/// Appends every bounded decoder result as a separate destination-encoder invocation.
fn append_decoded(chunks: &mut Vec<Vec<u32>>, encoding: Encoding, input: &[u8], state: &mut u32) {
    let mut input = input;
    while !input.is_empty() {
        chunks.push(encoding.decode_next(&mut input, 128, state).points);
    }
}

/// Parses an encoded word, returning its decoded bytes and consumed input span.
fn encoded_word(input: &[u8]) -> Option<(Encoding, Vec<u8>, usize)> {
    if input.len() < 6 || !input.starts_with(b"=?") { return None; }
    let charset_end = input[2..].iter().position(|&byte| byte == b'?')? + 2;
    let transfer = *input.get(charset_end + 1)?;
    if input.get(charset_end + 2) != Some(&b'?') { return None; }
    let encoding = Encoding::lookup_c_string(&input[2..charset_end])?;
    let start = charset_end + 3;
    let end = input[start..].windows(2).position(|pair| pair == b"?=").map(|index| start + index)
        .unwrap_or_else(|| input.len() - usize::from(start < input.len() && input.last() == Some(&b'?')));
    let bytes = match transfer {
        b'Q' | b'q' => qprint(&input[start..end]),
        b'B' | b'b' => base64(&input[start..end]),
        _ => return None,
    };
    Some((encoding, bytes, end + 2))
}

/// Recognizes the four whitespace bytes suppressed between valid encoded words.
fn whitespace(byte: u8) -> bool { matches!(byte, b'\r' | b'\n' | b'\t' | b' ') }

/// Decodes MIME Q transfer syntax, including consumed malformed hex pairs and soft breaks.
fn qprint(input: &[u8]) -> Vec<u8> {
    let mut output = Vec::with_capacity(input.len());
    let mut offset = 0;
    while offset < input.len() {
        let byte = input[offset];
        offset += 1;
        if byte == b'_' { output.push(b' '); continue; }
        if byte == b'=' && input.len() - offset >= 2 {
            let (second, third) = (input[offset], input[offset + 1]);
            offset += 2;
            if let (Some(high), Some(low)) = (hex(second), hex(third)) {
                output.push((high << 4) | low);
                continue;
            }
            if second == b'\r' {
                if third != b'\n' { offset -= 1; }
                continue;
            }
            if second == b'\n' { offset -= 1; continue; }
        }
        output.push(byte);
    }
    output
}

/// Maps a single MIME hexadecimal digit without Unicode or locale normalization.
fn hex(byte: u8) -> Option<u8> {
    match byte { b'0'..=b'9' => Some(byte - b'0'), b'A'..=b'F' => Some(byte - b'A' + 10),
        b'a'..=b'f' => Some(byte - b'a' + 10), _ => None }
}

/// Decodes permissive MIME Base64 while preserving invalid-byte replacement order.
fn base64(input: &[u8]) -> Vec<u8> {
    let mut output = Vec::with_capacity(input.len());
    let (mut bits, mut cache) = (0, 0u32);
    for &byte in input {
        if whitespace(byte) || byte == b'=' { continue; }
        let value = match byte {
            b'A'..=b'Z' => byte - b'A', b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52, b'+' => 62, b'/' => 63,
            _ => { output.push(b'?'); continue; }
        };
        bits += 6;
        cache = (cache << 6) | u32::from(value);
        if bits == 24 {
            output.extend_from_slice(&[(cache >> 16) as u8, (cache >> 8) as u8, cache as u8]);
            bits = 0;
            cache = 0;
        }
    }
    match bits {
        18 => output.extend_from_slice(&[(cache >> 10) as u8, (cache >> 2) as u8]),
        12 => output.push((cache >> 4) as u8),
        _ => {},
    }
    output
}
