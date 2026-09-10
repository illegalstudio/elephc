//! Purpose:
//! Selects mbstring encoding candidates using PHP's validation and character-frequency heuristic.
//!
//! Called from:
//! - Shared detection, conversion, and HTTP input after encoding-list normalization.
//!
//! Key details:
//! - Candidate ordering contributes a single-precision weight after each source string.
//! - Multiple strings are visited in reverse order with each candidate's decoder state retained.
//! - Transfer encodings are filtered by callers according to their own PHP contracts.
//! - Character frequencies are pinned to PHP's common-codepoint data, not host locale.

use crate::encoding::{Encoding, BAD_INPUT};

/// Selects the most likely interpretation of one byte string, preserving candidate-order ties.
pub fn guess(input: &[u8], candidates: &[Encoding], strict: bool, order_significant: bool) -> Option<Encoding> {
    guess_many(&[input], candidates, strict, order_significant)
}

/// Selects one encoding for independent strings using shared scores and persistent decoder state.
/// Callers retain string boundaries and their collection order; concatenating inputs changes
/// malformed-unit handling, BOM skipping, and repeated candidate-order weighting.
pub fn guess_many(inputs: &[&[u8]], candidates: &[Encoding], strict: bool, order_significant: bool) -> Option<Encoding> {
    let &first = candidates.first()?;
    if candidates.len() == 1 {
        return (!strict || inputs.iter().all(|input| first.decode(input).is_valid())).then_some(first);
    }
    if inputs.len() == 1 && inputs[0].is_empty() { return Some(first); }
    let mut best = None;
    for (index, &encoding) in candidates.iter().enumerate() {
        let multiplier = if order_significant { (1.0 + 0.3 * index as f64 / candidates.len() as f64) as f32 } else { 1.0 };
        let Some(score) = candidate_score(inputs, encoding, strict, multiplier) else { continue; };
        if best.is_none_or(|(_, previous)| score < previous) { best = Some((encoding, score)); }
    }
    best.map(|(encoding, _)| encoding)
}

/// Validates every original string, then scores bounded decoder calls in PHP's traversal order.
fn candidate_score(inputs: &[&[u8]], encoding: Encoding, strict: bool, multiplier: f32) -> Option<u64> {
    let mut score = 0u64;
    if encoding.detection_precheck() {
        for input in inputs {
            if !encoding.decode(input).is_valid() {
                if strict { return None; }
                score = score.saturating_add(500);
            }
        }
    }
    let mut state = 0;
    for input in inputs.iter().rev() {
        let mut remaining = without_bom(input, encoding);
        while !remaining.is_empty() {
            let decoded = encoding.decode_next(&mut remaining, 128, &mut state);
            for code in decoded.points {
                if strict && code == BAD_INPUT { return None; }
                score = score.saturating_add(demerits(code));
            }
        }
        score = (score as f64 * f64::from(multiplier)) as u64;
    }
    Some(score)
}

/// Removes only the BOMs explicitly skipped by PHP's detector for fixed byte-order encodings.
fn without_bom(input: &[u8], encoding: Encoding) -> &[u8] {
    let bom: &[u8] = match encoding.name() {
        "UTF-8" => b"\xef\xbb\xbf", "UTF-16BE" => b"\xfe\xff", "UTF-16LE" => b"\xff\xfe", _ => return input,
    };
    input.strip_prefix(bom).unwrap_or(input)
}

/// Assigns PHP's heuristic cost to malformed, supplementary, punctuation, and common/rare units.
fn demerits(code: u32) -> u64 {
    if code == BAD_INPUT { return 1000; }
    if code > 0xffff { return 40; }
    if (0x21..=0x2f).contains(&code) { return 6; }
    let common = include_bytes!("data/common.bin");
    if common[code as usize >> 3] & (1 << (code & 7)) != 0 { 1 } else { 30 }
}
