//! Purpose:
//! Compares encoding detection with PHP across candidate order, strictness, and complete byte pairs.
//!
//! Called from:
//! - `cargo test -p elephc-mbstring --test detect`.
//!
//! Key details:
//! - The oracle retains whether PHP's canonical encoding-list object disabled order weighting.
//! - Captured verdicts cover default settings, malformed data, filtering, and exact errors.

use std::io::{BufRead, BufReader};
use elephc_mbstring::{encoding::EncodingList, error::MbError, state::State};
use flate2::read::GzDecoder;
use serde_json::{json, Value};

/// Restores the binary source string or diagnostic from hexadecimal fixture bytes.
fn bytes(hex: &str) -> Vec<u8> {
    (0..hex.len()).step_by(2).map(|offset| u8::from_str_radix(&hex[offset..offset + 2], 16).unwrap()).collect()
}

/// Checks independently captured guesses, false results, and PHP argument errors.
#[test]
fn encoding_detection_matches_php() {
    let fixture = include_bytes!("fixtures/detect.jsonl.gz");
    let reader = BufReader::new(GzDecoder::new(fixture.as_slice()));
    let mut state = State::default();
    let mut count = 0;
    for line in reader.lines() {
        let case: Value = serde_json::from_str(&line.unwrap()).unwrap();
        let input = bytes(case["input"].as_str().unwrap());
        let names = case["list"].as_array().map(|list| list.iter().map(|name| name.as_str().unwrap().as_bytes().to_vec()).collect::<Vec<_>>());
        let csv = case["list"].get("bytes").map(|hex| bytes(hex.as_str().unwrap()));
        let list = names.as_deref().map(EncodingList::Array).or_else(|| csv.as_deref().map(EncodingList::CommaSeparated));
        state.set_strict_detection(case["default_strict"].as_bool().unwrap());
        let result = state.detect_encoding(&input, list, case["strict"].as_bool(), case["ordered"].as_bool().unwrap());
        if let Some(error) = case.get("error") {
            assert_eq!(error[0], "ValueError", "{case}");
            let actual = match result.unwrap_err() {
                MbError::Value(message) => message.into_bytes(), MbError::ValueBytes(message) => message,
                error => panic!("unexpected error {error:?}"),
            };
            assert_eq!(actual, bytes(error[1].as_str().unwrap()), "{case}");
        } else {
            let actual = result.unwrap().map(|encoding| json!(encoding.name())).unwrap_or(json!(false));
            assert_eq!(actual, case["result"], "{case}");
        }
        count += 1;
    }
    assert!(count > 400_000, "incomplete detection fixture: {count}");
}
