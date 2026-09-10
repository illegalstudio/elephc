//! Purpose:
//! Implements mbstring numeric-entity encoding and decoding over shared character codecs.
//!
//! Called from:
//! - mb_encode_numericentity and mb_decode_numericentity backend adapters.
//!
//! Key details:
//! - Map values use wrapping 32-bit arithmetic; decoding deliberately ignores each mask.
//! - The first matching map range wins, and a semicolon is optional when decoding.
//! - Adapters coerce map values to PHP integers before invoking these operations.

use crate::encoding::{Encoding, Substitute};
use crate::error::{MbError, MbResult};

/// Ordered ranges with the offset and mask used by PHP's numeric-entity contract.
struct Map(Vec<[u32; 4]>);

impl Map {
    /// Validates complete four-element ranges, retaining PHP's unsigned casts and order.
    fn new(values: &[i64], function: &str) -> MbResult<Self> {
        if values.len() % 4 != 0 {
            return Err(MbError::argument(function, 2, "map", "must have a multiple of 4 elements"));
        }
        Ok(Self(values.chunks_exact(4).map(|range| [range[0] as u32, range[1] as u32, range[2] as u32, range[3] as u32]).collect()))
    }

    /// Finds the first range containing a decoded codepoint, then applies its offset and mask.
    fn encode(&self, code: u32) -> Option<u32> {
        self.0.iter().find_map(|&[low, high, offset, mask]| {
            (low..=high).contains(&code).then(|| code.wrapping_add(offset) & mask)
        })
    }

    /// Reverses an offset if the resulting codepoint belongs to that range, without masking.
    fn decode(&self, value: u32) -> Option<u32> {
        self.0.iter().find_map(|&[low, high, offset, _]| {
            let code = value.wrapping_sub(offset);
            (low..=high).contains(&code).then_some(code)
        })
    }
}

/// Replaces matching codepoints with decimal or uppercase hexadecimal numeric references.
pub fn encode_numericentity(input: &[u8], map: &[i64], encoding: Encoding, substitution: Substitute, hex: bool) -> MbResult<Vec<u8>> {
    let map = Map::new(map, "mb_encode_numericentity")?;
    if input.is_empty() { return Ok(Vec::new()); }
    let decoded = encoding.decode_buffer(input, 32);
    let mut start = 0;
    let mut chunks = Vec::new();
    for &end in &decoded.case_batches {
        let mut output = Vec::new();
        for &code in &decoded.points[start..end] {
            if let Some(value) = map.encode(code) {
                let entity = if hex { format!("&#x{value:X};") } else { format!("&#{value};") };
                output.extend(entity.bytes().map(u32::from));
            } else { output.push(code); }
        }
        chunks.push(output);
        start = end;
    }
    Ok(encoding.encode_chunks(&chunks, substitution))
}

/// Decodes references permitted by the map, preserving malformed or unmatched references.
pub fn decode_numericentity(input: &[u8], map: &[i64], encoding: Encoding, substitution: Substitute) -> MbResult<Vec<u8>> {
    let map = Map::new(map, "mb_decode_numericentity")?;
    if input.is_empty() { return Ok(Vec::new()); }
    let decoded = encoding.decode(input);
    if encoding.needs_transform_batches() {
        let mut chunks = Vec::new();
        let (mut start, mut pending) = (0, Vec::new());
        while start < decoded.points.len() {
            let end = encoding.mobile_batch_end(&decoded, start, 127 - pending.len());
            pending.extend_from_slice(&decoded.points[start..end]);
            let (output, consumed) = decode_part(&pending, &map, end < decoded.points.len());
            pending.drain(..consumed);
            chunks.push(output);
            start = end;
        }
        debug_assert!(pending.is_empty());
        return Ok(encoding.encode_chunks(&chunks, substitution));
    }
    Ok(encoding.encode(&decode_part(&decoded.points, &map, false).0, substitution))
}

/// Converts one decoder batch, deferring a potential reference split across its final boundary.
fn decode_part(input: &[u32], map: &Map, more: bool) -> (Vec<u32>, usize) {
    let mut output = Vec::new();
    let mut position = 0;
    while position < input.len() {
        if more && partial_reference(&input[position..]) { break; }
        if let Some((code, end)) = reference(input, position, map) {
            output.push(code);
            position = end;
        } else {
            output.push(input[position]);
            position += 1;
        }
    }
    (output, position)
}

/// Reports a bounded reference prefix that PHP retries after filling the next decoder buffer.
fn partial_reference(input: &[u32]) -> bool {
    if input[0] != u32::from(b'&') { return false; }
    if input.len() == 1 { return true; }
    if input[1] != u32::from(b'#') { return false; }
    let hex = input.get(2) == Some(&u32::from(b'x'));
    let first = if hex { 3 } else { 2 };
    input.len() - first <= if hex { 8 } else { 10 } && input[first..].iter().all(|&code| digit(code, hex).is_some())
}

/// Recognizes one bounded numeric reference, including PHP's decimal-overflow edge case.
fn reference(input: &[u32], start: usize, map: &Map) -> Option<(u32, usize)> {
    if input.get(start..start + 2)? != [u32::from(b'&'), u32::from(b'#')] { return None; }
    let hex = input.get(start + 2) == Some(&u32::from(b'x'));
    let first = start + if hex { 3 } else { 2 };
    let mut end = first;
    let mut value = 0u32;
    let mut valid = true;
    while let Some(digit) = input.get(end).and_then(|&code| digit(code, hex)) {
        if !hex && value > 0x1999_9999 { valid = false; }
        value = value.wrapping_mul(if hex { 16 } else { 10 }).wrapping_add(digit);
        end += 1;
    }
    if !valid || end == first || end - first > if hex { 8 } else { 10 } { return None; }
    let code = map.decode(value)?;
    if input.get(end) == Some(&u32::from(b';')) { end += 1; }
    Some((code, end))
}

/// Parses decimal digits or the optional ASCII hexadecimal alphabet without Unicode coercion.
fn digit(code: u32, hex: bool) -> Option<u32> {
    match code {
        0x30..=0x39 => Some(code - 0x30),
        0x41..=0x46 if hex => Some(code - 0x41 + 10),
        0x61..=0x66 if hex => Some(code - 0x61 + 10),
        _ => None,
    }
}
