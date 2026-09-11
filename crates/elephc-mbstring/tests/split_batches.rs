//! Purpose:
//! Compares full mb_str_split arrays across decoder boundaries with PHP's captured output.
//!
//! Called from:
//! - Cargo's focused mbstring split-batches integration test binary.
//!
//! Key details:
//! - All 79 encodings include long inputs, incomplete units, and substitution modes.
//! - Every binary element is compared independently, including array boundaries.

use std::io::{BufRead, BufReader};
use elephc_mbstring::{encoding::{Encoding, Substitute, SubstituteMode}, text};
use flate2::read::GzDecoder;
use serde_json::Value;

/// Decodes one byte-exact fixture string without assuming valid UTF-8.
fn bytes(hex: &str) -> Vec<u8> {
    (0..hex.len()).step_by(2).map(|index| u8::from_str_radix(&hex[index..index + 2], 16).unwrap()).collect()
}

/// Verifies raw and converted splitting retain PHP's chunk boundaries and replacement behavior.
#[test]
fn split_decoder_batches_match_php() {
    let fixture = include_bytes!("fixtures/split-batches.jsonl.gz");
    let reader = BufReader::new(GzDecoder::new(fixture.as_slice()));
    let mut count = 0;
    for line in reader.lines() {
        let case: Value = serde_json::from_str(&line.unwrap()).unwrap();
        let encoding = Encoding::lookup(case["encoding"].as_str().unwrap().as_bytes()).unwrap();
        let mode = match case["substitute"].as_str() { Some("none") => SubstituteMode::None,
            Some("long") => SubstituteMode::Long, Some("entity") => SubstituteMode::Entity, _ => SubstituteMode::Character };
        let substitute = Substitute { mode, character: case["substitute"].as_u64().unwrap_or(0xfffd) as u32 };
        let input = bytes(case["input"].as_str().unwrap());
        let output = text::str_split(&input, case["length"].as_i64().unwrap(), encoding, substitute).unwrap();
        let expected: Vec<_> = case["output"].as_array().unwrap().iter().map(|part| bytes(part.as_str().unwrap())).collect();
        assert_eq!(output, expected, "{case}");
        count += 1;
    }
    assert_eq!(count, 35076);
}
