//! Purpose:
//! Reconstructs UTF-16 decoder batches used by PHP's pinned x86_64 compatibility baseline.
//!
//! Called from:
//! - Unicode decoding for casing and the catalog's 128-word conversion decoder.
//!
//! Key details:
//! - Scalar decoding reserves one word; the baseline's vector path consumes 16-unit blocks.
//! - This portable scan preserves observable partitions without requiring SIMD instructions.

/// Returns decoded-point boundaries after BOM handling and PHP's scalar/vector reservations.
pub(super) fn ends(input: &[u8], mut little: bool, auto: bool, capacity: usize) -> Vec<usize> {
    let mut offset = 0;
    if auto && input.len() >= 2 {
        match &input[..2] {
            [0xff, 0xfe] => { little = true; offset = 2; }
            [0xfe, 0xff] => offset = 2,
            _ => {}
        }
    }
    let mut ends = Vec::new();
    let mut total = 0;
    while offset < input.len() {
        let mut count = 0;
        let vector = input.len() - offset >= 32 && capacity >= 16;
        if vector {
            while input.len() - offset >= 32 && capacity - count >= 16 {
                let prefix = (0..16).take_while(|index| !surrogate(unit(input, offset + index * 2, little))).count();
                if prefix > 0 {
                    offset += prefix * 2;
                    count += prefix;
                    continue;
                }
                let mut run = (0..16).take_while(|index| surrogate(unit(input, offset + index * 2, little))).count();
                while run > 0 {
                    let first = unit(input, offset, little);
                    let paired = first < 0xdc00 && offset + 3 < input.len()
                        && (0xdc00..=0xdfff).contains(&unit(input, offset + 2, little));
                    let consumed = if paired { 2 } else { 1 };
                    offset += consumed * 2;
                    count += 1;
                    run = run.saturating_sub(consumed);
                }
            }
        }
        if !vector || (offset < input.len() && capacity - count >= 4) {
            let limit = capacity - 1;
            while offset + 1 < input.len() && count < limit {
                let first = unit(input, offset, little);
                offset += 2;
                count += 1;
                if (0xd800..=0xdbff).contains(&first) && offset + 1 < input.len() {
                    let second = unit(input, offset, little);
                    if !(0xd800..=0xdbff).contains(&second) {
                        offset += 2;
                        if !(0xdc00..=0xdfff).contains(&second) { count += 1; }
                    }
                }
            }
            if offset + 1 == input.len() && count < limit { offset += 1; count += 1; }
        }
        total += count;
        ends.push(total);
    }
    ends
}

/// Reads an available UTF-16 unit in the byte order selected by the encoding or BOM.
fn unit(input: &[u8], offset: usize, little: bool) -> u16 {
    let pair = [input[offset], input[offset + 1]];
    if little { u16::from_le_bytes(pair) } else { u16::from_be_bytes(pair) }
}

/// Identifies either half of a UTF-16 surrogate pair.
fn surrogate(code: u16) -> bool { (0xd800..=0xdfff).contains(&code) }
