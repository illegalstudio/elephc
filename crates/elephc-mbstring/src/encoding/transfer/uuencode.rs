//! Purpose:
//! Implements PHP's legacy UUENCODE transfer format over decoded low-byte values.
//!
//! Called from:
//! - The shared transfer encoding dispatcher.
//!
//! Key details:
//! - The decoder searches for a begin header anywhere and silently discards incomplete groups.
//! - Output uses 45-byte lines and PHP's fixed filename header without an end trailer.

use crate::encoding::Decoded;

/// Decodes complete data groups with the original permissive header and trailing-byte rules.
pub(super) fn decode(input: &[u8], capacity: usize) -> Decoded {
    decode_state(input, capacity, &mut 0)
}

/// Retains PHP's packed decoder mode and line count between separate MIME encoded words.
pub(super) fn decode_state(input: &[u8], capacity: usize, saved: &mut u32) -> Decoded {
    decode_mode(input, capacity, saved, false).0
}

/// Returns one transfer-decoder buffer and the exact consumed input byte count.
pub(super) fn decode_next(input: &[u8], capacity: usize, saved: &mut u32) -> (Decoded, usize) {
    decode_mode(input, capacity, saved, true)
}

/// Keeps bounded source reads and complete-stream conversion on the same token parser.
fn decode_mode(input: &[u8], capacity: usize, saved: &mut u32, single: bool) -> (Decoded, usize) {
    let mut output = Decoded::default();
    let (mut offset, mut state, mut remaining, mut batch) = (0, *saved & 255, (*saved >> 8) as usize, 0);
    while offset < input.len() {
        if output.points.len() - batch > capacity - 3 {
            if single { break; }
            output.case_batches.push(output.points.len());
            batch = output.points.len();
        }
        let start = offset;
        let byte = input[offset];
        offset += 1;
        match state {
            0 => {
                if byte == b'b' && input[offset..].starts_with(b"egin ") {
                    offset += 5;
                    while offset < input.len() {
                        offset += 1;
                        if input[offset - 1] == b'\n' { break; }
                    }
                    state = 3;
                }
            }
            3 => { remaining = usize::from(byte.wrapping_sub(32) & 63); state = 4; }
            4 => {
                if input.len() - offset < 4 { offset = input.len(); break; }
                let digits = [byte, input[offset], input[offset + 1], input[offset + 2]].map(|byte| u32::from(byte.wrapping_sub(32) & 63));
                offset += 3;
                let [a, b, c, d] = digits;
                for code in [((a << 2) | (b >> 4)) & 255, ((b << 4) | (c >> 2)) & 255, ((c << 6) | d) & 255]
                    .into_iter().take(remaining.min(3)) { output.push(code, start); }
                remaining = remaining.saturating_sub(3);
                if remaining == 0 { state = 8; }
            }
            8 => state = 3,
            _ => {},
        }
    }
    if output.points.len() != batch { output.case_batches.push(output.points.len()); }
    *saved = ((remaining as u32) << 8) | state;
    (output, offset)
}

/// Preserves UUENCODE's partial-group padding and line-length repairs across encoder calls.
pub(super) fn encode_chunks(chunks: &[Vec<u32>]) -> Vec<u8> {
    encode_prefix(chunks, true)
}

/// Replays bounded MIME trial chunks with optional final padding and line-length repair.
pub(super) fn encode_prefix(chunks: &[Vec<u32>], finish: bool) -> Vec<u8> {
    encode_with_final_policy(chunks, finish, true)
}

/// Flushes at the final actual decoder batch for an output-handler feed.
pub(super) fn encode_output_chunks(chunks: &[Vec<u32>], finish: bool) -> Vec<u8> {
    encode_with_final_policy(chunks, finish, false)
}

/// Shares UUENCODE state transitions while distinguishing MIME's extra final call.
fn encode_with_final_policy(chunks: &[Vec<u32>], finish: bool, separate_final: bool) -> Vec<u8> {
    if chunks.is_empty() && !finish { return Vec::new(); }
    let mut output = b"begin 0644 filename\n".to_vec();
    let (mut started, mut encoded, mut cached, mut cached_bits) = (false, 0usize, 0u32, 0);
    for (call, input) in chunks.iter().map(Vec::as_slice).chain((finish && separate_final).then_some(&[][..])).enumerate() {
        let end = finish && if separate_final { call == chunks.len() } else { call + 1 == chunks.len() };
        let mut offset = 0;
        if !started {
            output.push(input.len().min(45) as u8 + 32);
            started = true;
        } else if input.is_empty() && end && encoded == 0 && cached_bits == 0 {
            output.pop();
            break;
        } else {
            let length_position = output.len() - encoded * 4 / 3 - 1
                - match cached_bits { 2 => 1, 4 => 2, _ => 0 };
            output[length_position] = (encoded + input.len() + match cached_bits { 2 => 1, 4 => 2, _ => 0 }).min(45) as u8 + 32;
            if cached_bits != 0 {
                let second = input.get(offset).copied().unwrap_or(0);
                offset += usize::from(offset < input.len());
                if cached_bits == 2 {
                    let third = input.get(offset).copied().unwrap_or(0);
                    offset += usize::from(offset < input.len());
                    output.extend_from_slice(&[digit((cached << 4) | ((second >> 4) & 15)),
                        digit(((second & 15) << 2) | ((third >> 6) & 3)), digit(third & 63)]);
                } else {
                    output.extend_from_slice(&[digit((cached << 2) | ((second >> 6) & 3)), digit(second & 63)]);
                }
                cached_bits = 0;
                cached = 0;
                line_break(&mut output, &mut encoded, input.len() - offset, end);
            }
        }
        while offset < input.len() {
            let first = input[offset];
            offset += 1;
            output.push(digit((first >> 2) & 63));
            if offset == input.len() && !end { cached = first & 3; cached_bits = 2; break; }
            let second = input.get(offset).copied().unwrap_or(0);
            offset += usize::from(offset < input.len());
            output.push(digit(((first & 3) << 4) | ((second >> 4) & 15)));
            if offset == input.len() && !end { cached = second & 15; cached_bits = 4; break; }
            let third = input.get(offset).copied().unwrap_or(0);
            offset += usize::from(offset < input.len());
            output.extend_from_slice(&[digit(((second & 15) << 2) | ((third >> 6) & 3)), digit(third & 63)]);
            line_break(&mut output, &mut encoded, input.len() - offset, end);
        }
        if encoded != 0 && end { output.push(b'\n'); }
    }
    output
}

/// Updates a UUENCODE line after one complete or padded group of three bytes.
fn line_break(output: &mut Vec<u8>, encoded: &mut usize, remaining: usize, end: bool) {
    *encoded += 3;
    if *encoded >= 45 {
        output.push(b'\n');
        if remaining != 0 || !end { output.push(remaining.min(45) as u8 + 32); }
        *encoded = 0;
    }
}

/// Converts a six-bit group into the canonical UUENCODE alphabet.
fn digit(value: u32) -> u8 { if value == 0 { b'`' } else { value as u8 + 32 } }

/// Encodes low-byte values with canonical backticks, per-line lengths, and PHP's fixed header.
pub(super) fn encode(input: &[u32]) -> Vec<u8> {
    let mut output = b"begin 0644 filename\n".to_vec();
    if input.is_empty() { output.push(b' '); return output; }
    for line in input.chunks(45) {
        output.push(line.len() as u8 + 32);
        for group in line.chunks(3) {
            let a = group[0] as u8;
            let b = group.get(1).copied().unwrap_or(0) as u8;
            let c = group.get(2).copied().unwrap_or(0) as u8;
            for digit in [a >> 2, ((a & 3) << 4) | (b >> 4), ((b & 15) << 2) | (c >> 6), c & 63] {
                output.push(if digit == 0 { b'`' } else { digit + 32 });
            }
        }
        output.push(b'\n');
    }
    output
}
