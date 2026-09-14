//! Purpose:
//! Compares complete numeric-entity operations with the pinned PHP oracle.
//!
//! Called from:
//! - `cargo test -p elephc-mbstring --test entities`.
//!
//! Key details:
//! - Maps cover unsigned casts, range order, masks, and malformed lengths.
//! - Byte strings include transfer codecs, broken references, and KDDI batch boundaries.

use std::io::{BufRead, BufReader};
use elephc_mbstring::{encoding::{Encoding, Substitute, SubstituteMode}, error::MbError, text};
use flate2::read::GzDecoder;
use serde_json::Value;

/// Decodes the lossless hexadecimal representation of a PHP byte string.
fn bytes(hex: &str) -> Vec<u8> {
    (0..hex.len()).step_by(2).map(|index| u8::from_str_radix(&hex[index..index + 2], 16).unwrap()).collect()
}

/// Checks complete outputs and PHP exceptions for every captured numeric-entity request.
#[test]
fn numeric_entities_match_php() {
    let fixture = include_bytes!("fixtures/entities.jsonl.gz");
    let reader = BufReader::new(GzDecoder::new(fixture.as_slice()));
    let mut count = 0;
    for line in reader.lines() {
        let case: Value = serde_json::from_str(&line.unwrap()).unwrap();
        let encoding = Encoding::lookup(case["encoding"].as_str().unwrap().as_bytes()).unwrap();
        let input = bytes(case["input"].as_str().unwrap());
        let map = case["map"].as_array().unwrap().iter().map(|value| value.as_i64().unwrap()).collect::<Vec<_>>();
        let mode = match case["substitute"].as_str() {
            Some("none") => SubstituteMode::None, Some("long") => SubstituteMode::Long,
            Some("entity") => SubstituteMode::Entity, None => SubstituteMode::Character,
            other => panic!("unexpected substitution mode {other:?}"),
        };
        let substitute = Substitute { mode, character: 0xfffd };
        let result = if case["decode"].as_bool().unwrap() {
            text::decode_numericentity(&input, &map, encoding, substitute)
        } else {
            text::encode_numericentity(&input, &map, encoding, substitute, case["hex"].as_bool().unwrap())
        };
        if let Some(error) = case.get("error") {
            assert_eq!(error[0], "ValueError", "{case}");
            assert_eq!(result, Err(MbError::Value(error[1].as_str().unwrap().to_owned())), "{case}");
        } else {
            assert_eq!(result.unwrap(), bytes(case["output"].as_str().unwrap()), "{case}");
        }
        count += 1;
    }
    assert!(count > 100_000, "incomplete entity fixture: {count}");
}
