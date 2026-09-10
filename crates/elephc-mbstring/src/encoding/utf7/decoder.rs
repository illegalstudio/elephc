//! Purpose:
//! Decodes modified Base64 sections with PHP's UTF-16, padding, and termination behavior.
//!
//! Called from:
//! - `super::Utf7::decode`.
//!
//! Key details:
//! - Conversion accepts some direct ASCII bytes which strict UTF-7 validation rejects.
//! - Explicit batch ends preserve contextual case behavior across five-word reservations.

use crate::encoding::{Decoded, BAD_INPUT};

/// Decodes a complete UTF-7 string, retaining invalid units and additional validation failures.
pub(super) fn decode(input: &[u8], imap: bool) -> Decoded {
    decode_buffer(input, imap, 64)
}

/// Retains PHP's five-word reservations for the requested conversion scratch-buffer capacity.
pub(super) fn decode_buffer(input: &[u8], imap: bool, capacity: usize) -> Decoded {
    decode_state(input, imap, capacity, &mut 0)
}

/// Preserves the selected Base64 mode and PHP's pending surrogate across MIME words.
pub(super) fn decode_state(input: &[u8], imap: bool, capacity: usize, state: &mut u32) -> Decoded {
    decode_mode(input, imap, capacity, state, false).0
}

/// Decodes one PHP scratch-buffer call and returns its consumed byte count and saved state.
pub(super) fn decode_next(input: &[u8], imap: bool, capacity: usize, state: &mut u32) -> (Decoded, usize) {
    decode_mode(input, imap, capacity, state, true)
}

/// Shares complete-stream decoding with the bounded MIME source reader.
fn decode_mode(input: &[u8], imap: bool, capacity: usize, state: &mut u32, single: bool) -> (Decoded, usize) {
    let mut output = Decoded::default();
    let (mut offset, mut batch) = (0, 0);
    let (mut active, mut bits, mut count, mut digits) = (*state & 1 != 0, 0u32, 0u8, 0usize);
    let mut surrogate = if (*state >> 1) as u16 != 0 { Some((*state >> 1) as u16) } else { None };
    while offset < input.len() {
        let emitted = output.points.len() - batch;
        if (!active && emitted >= capacity - usize::from(imap))
            || (active && digits % 8 == 0 && emitted > capacity - 5)
        {
            if single { break; }
            output.case_batches.push(output.points.len());
            batch = output.points.len();
        }
        let byte = input[offset];
        let position = offset;
        offset += 1;
        if !active {
            if byte == if imap { b'&' } else { b'+' } {
                if input.get(offset) == Some(&b'-') {
                    output.push(u32::from(byte), position);
                    offset += 1;
                } else if imap || offset < input.len() {
                    active = true;
                    digits = 0;
                    if !imap && super::digit(input[offset], false).is_none() { output.validation_error = true; }
                }
            } else if if imap { (0x20..=0x7e).contains(&byte) } else { byte < 128 } {
                output.push(u32::from(byte), position);
                if !imap && !super::direct(u32::from(byte), false) && !super::optional_direct(byte) {
                    output.validation_error = true;
                }
            } else {
                output.push(BAD_INPUT, position);
            }
            continue;
        }
        if let Some(digit) = super::digit(byte, imap) {
            bits = (bits << 6) | digit;
            count += 6;
            digits += 1;
            if count >= 16 {
                count -= 16;
                let code = (bits >> count) as u16;
                bits &= (1 << count) - 1;
                unit(code, imap, &mut surrogate, position, &mut output);
            }
        } else {
            let abrupt = !matches!(count, 0 | 2 | 4) || bits != 0 || surrogate.is_some();
            if abrupt || (imap && byte != b'-') { output.push(BAD_INPUT, position); }
            if !imap && byte != b'-' {
                if byte < 128 { offset -= 1; } else { output.push(BAD_INPUT, position); }
            }
            active = false;
            bits = 0;
            count = 0;
            surrogate = None;
        }
    }
    if active && offset == input.len() {
        let position = input.len().saturating_sub(1);
        if !matches!(count, 0 | 2 | 4) {
            output.push(BAD_INPUT, position);
            active = false;
            surrogate = None;
        } else {
            if bits != 0 || (surrogate.is_some() && count != 0) {
                output.push(BAD_INPUT, position);
                if !imap { surrogate = None; }
            }
            if imap { output.push(BAD_INPUT, position); }
        }
    }
    if offset == input.len() && !imap && surrogate.is_some() { output.push(BAD_INPUT, input.len().saturating_sub(1)); }
    *state = (u32::from(surrogate.unwrap_or(0)) << 1) | u32::from(active);
    if output.points.len() != batch { output.case_batches.push(output.points.len()); }
    (output, offset)
}

/// Combines UTF-16 surrogate pairs and rejects printable ASCII hidden in IMAP Base64.
fn unit(code: u16, imap: bool, surrogate: &mut Option<u16>, offset: usize, output: &mut Decoded) {
    if let Some(first) = surrogate.take() {
        if (0xdc00..=0xdfff).contains(&code) {
            output.push(0x10000 + (u32::from(first & 0x3ff) << 10) + u32::from(code & 0x3ff), offset);
            return;
        }
        output.push(BAD_INPUT, offset);
    }
    if (0xd800..=0xdbff).contains(&code) {
        *surrogate = Some(code);
    } else if (0xdc00..=0xdfff).contains(&code) || (imap && (0x20..=0x7e).contains(&code) && code != u16::from(b'&')) {
        output.push(BAD_INPUT, offset);
    } else {
        output.push(u32::from(code), offset);
    }
}
