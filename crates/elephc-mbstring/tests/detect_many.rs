//! Purpose:
//! Compares multi-string encoding detection with independent PHP conversion oracles.
//!
//! Called from:
//! - `cargo test -p elephc-mbstring --test detect_many`.
//!
//! Key details:
//! - Inputs retain source boundaries, order, malformed units, and stateful shift sequences.
//! - The original mb_list_encodings array disables order weighting in the PHP oracle.

use std::io::{BufRead, BufReader};
use elephc_mbstring::{detect::guess_many, encoding::Encoding};
use flate2::read::GzDecoder;
use serde_json::{json, Value};

/// Restores an independently captured binary string from its hexadecimal fixture form.
fn bytes(hex: &str) -> Vec<u8> {
    (0..hex.len()).step_by(2).map(|offset| u8::from_str_radix(&hex[offset..offset + 2], 16).unwrap()).collect()
}

/// Checks shared scores, reverse source traversal, repeated weights, and decoder-state retention.
#[test]
fn multiple_string_detection_matches_php() {
    let fixture = include_bytes!("fixtures/detect_many.jsonl.gz");
    let reader = BufReader::new(GzDecoder::new(fixture.as_slice()));
    let mut count = 0;
    for line in reader.lines() {
        let case: Value = serde_json::from_str(&line.unwrap()).unwrap();
        let inputs = case["inputs"].as_array().unwrap().iter().map(|hex| bytes(hex.as_str().unwrap())).collect::<Vec<_>>();
        let candidates = case["candidates"].as_array().unwrap().iter().map(|name| Encoding::lookup(name.as_str().unwrap().as_bytes()).unwrap()).collect::<Vec<_>>();
        let inputs = inputs.iter().map(Vec::as_slice).collect::<Vec<_>>();
        let result = guess_many(&inputs, &candidates, case["strict"].as_bool().unwrap(), case["ordered"].as_bool().unwrap());
        let actual = result.map(|encoding| json!(encoding.name())).unwrap_or(json!(false));
        assert_eq!(actual, case["result"], "{case}");
        count += 1;
    }
    assert_eq!(count, 9260, "incomplete multi-string fixture");
}
