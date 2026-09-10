//! Purpose:
//! Compares automatic conversion, source filtering, and diagnostic ordering with PHP.
//!
//! Called from:
//! - `cargo test -p elephc-mbstring --test detected_conversion`.
//!
//! Key details:
//! - Single transfer sources bypass detection; mixed source lists filter transfer encodings.
//! - Destination lookup can emit a deprecation before a source-list error is raised.

use std::io::{BufRead, BufReader};
use elephc_mbstring::{encoding::EncodingList, error::{MbError, MbResult}, state::State};
use flate2::read::GzDecoder;
use serde_json::{json, Value};

/// Restores binary input and diagnostics from hexadecimal JSON strings.
fn bytes(hex: &str) -> Vec<u8> {
    (0..hex.len()).step_by(2).map(|offset| u8::from_str_radix(&hex[offset..offset + 2], 16).unwrap()).collect()
}

/// Encodes arbitrary bytes without requiring valid UTF-8.
fn hex(input: &[u8]) -> String { input.iter().map(|byte| format!("{byte:02x}")).collect() }

/// Replays the shared preparation and conversion operations in PHP's observable order.
fn execute(case: &Value, warnings: &mut Vec<Value>) -> MbResult<Value> {
    let mut state = State::default();
    state.set_internal_encoding(case["internal"].as_str().unwrap().as_bytes())?;
    state.set_strict_detection(case["strict"].as_bool().unwrap());
    if let Some(mode) = case["substitute"].as_str() { state.set_substitute_mode(mode.as_bytes())?; }
    else { state.set_substitute_codepoint(case["substitute"].as_i64().unwrap())?; }
    let destination = state.resolve_encoding(Some(&bytes(case["to"].as_str().unwrap())), "mb_convert_encoding", 2, "to_encoding")?;
    if let Some(message) = destination.deprecation {
        warnings.push(json!([8192, hex(format!("mb_convert_encoding(): {message}").as_bytes())]));
    }
    let names = case["from"].as_array().map(|array| array.iter().map(|name| name.as_str().unwrap().as_bytes().to_vec()).collect::<Vec<_>>());
    let csv = case["from"].get("bytes").map(|value| bytes(value.as_str().unwrap()));
    let list = names.as_deref().map(EncodingList::Array).or_else(|| csv.as_deref().map(EncodingList::CommaSeparated));
    let sources = state.conversion_sources(list)?;
    let input = bytes(case["input"].as_str().unwrap());
    if let Some(output) = sources.convert(&input, destination.encoding, state.strict_detection(), state.substitute()) {
        Ok(json!(hex(&output)))
    } else {
        warnings.push(json!([2, hex(b"mb_convert_encoding(): Unable to detect character encoding")]));
        Ok(json!(false))
    }
}

/// Checks exact output, warnings, and failure order against the independent oracle.
#[test]
fn automatic_conversion_matches_php() {
    let fixture = include_bytes!("fixtures/detected-conversion.jsonl.gz");
    let reader = BufReader::new(GzDecoder::new(fixture.as_slice()));
    let mut count = 0;
    for line in reader.lines() {
        let case: Value = serde_json::from_str(&line.unwrap()).unwrap();
        let mut warnings = Vec::new();
        let result = execute(&case, &mut warnings);
        if let Some(error) = case.get("error") {
            assert_eq!(error[0], "ValueError", "{case}");
            let actual = match result.unwrap_err() {
                MbError::Value(message) => message.into_bytes(), MbError::ValueBytes(message) => message,
                error => panic!("unexpected error {error:?}"),
            };
            assert_eq!(actual, bytes(error[1].as_str().unwrap()), "{case}");
        } else { assert_eq!(result.unwrap(), case["result"], "{case}"); }
        assert_eq!(json!(warnings), case["warnings"], "diagnostics {case}");
        count += 1;
    }
    assert!(count > 50_000, "incomplete automatic conversion fixture: {count}");
}
