//! Purpose:
//! Compares observable conversion and contextual casing partitions with PHP's decoder baseline.
//!
//! Called from:
//! - `cargo test -p elephc-mbstring --test batches`.
//!
//! Key details:
//! - Mobile JIS emoji lookahead ends at the source decoder's 128-word partitions.
//! - UTF-16 checks valid and malformed surrogates around vector and scalar batch boundaries.

use std::io::{BufRead, BufReader};
use elephc_mbstring::{encoding::{Encoding, Substitute, SubstituteMode}, text, unicode::CaseMode};
use flate2::read::GzDecoder;
use serde_json::Value;

/// Restores an oracle's exact encoded bytes independently of its JSON string representation.
fn bytes(hex: &str) -> Vec<u8> {
    (0..hex.len()).step_by(2).map(|index| u8::from_str_radix(&hex[index..index + 2], 16).unwrap()).collect()
}

/// Covers source-specific reservations, mobile lookahead, and UTF-16 contextual sigma windows.
#[test]
fn decoder_batch_boundaries_match_php() {
    let reader = BufReader::new(GzDecoder::new(include_bytes!("fixtures/batches.jsonl.gz").as_slice()));
    let mobile = Encoding::lookup(b"ISO-2022-JP-MOBILE#KDDI").unwrap();
    for line in reader.lines() {
        let case: Value = serde_json::from_str(&line.unwrap()).unwrap();
        let encoding = Encoding::lookup(case["encoding"].as_str().unwrap().as_bytes()).unwrap();
        let input = bytes(case["input"].as_str().unwrap());
        let mode = match case["substitute"].as_str() { Some("none") => SubstituteMode::None,
            Some("long") => SubstituteMode::Long, Some("entity") => SubstituteMode::Entity, _ => SubstituteMode::Character };
        let substitute = Substitute { mode, character: case["substitute"].as_u64().unwrap_or(0xfffd) as u32 };
        let output = if case["kind"] == "convert" { text::convert_encoding(&input, encoding, mobile, substitute) }
            else if case["kind"] == "kana" { text::convert_kana(&input, text::KanaMode::default(), encoding, substitute) }
            else { text::convert_case(&input, CaseMode::Lower, encoding, substitute) };
        assert_eq!(output, bytes(case["output"].as_str().unwrap()), "{case}");
    }
}
