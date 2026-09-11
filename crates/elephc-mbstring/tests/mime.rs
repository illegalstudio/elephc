//! Purpose:
//! Compares MIME header operations with lossless fixtures from the pinned PHP baseline.
//!
//! Called from:
//! - `cargo test -p elephc-mbstring --test mime`.
//!
//! Key details:
//! - Covers all internal encodings, permissive transfer syntax, and inter-word codec state.

use std::io::{BufRead, BufReader};
use elephc_mbstring::{encoding::Encoding, mime};
use flate2::read::GzDecoder;
use serde_json::Value;

/// Restores a byte string captured without assuming UTF-8 validity.
fn bytes(hex: &str) -> Vec<u8> {
    (0..hex.len()).step_by(2).map(|index| u8::from_str_radix(&hex[index..index + 2], 16).unwrap()).collect()
}

/// Checks all captured decoding outputs, including legacy acceptance of malformed words.
#[test]
fn mime_decode_matches_php() {
    let fixture = include_bytes!("fixtures/mime_decode.jsonl.gz");
    let reader = BufReader::new(GzDecoder::new(fixture.as_slice()));
    let (mut count, mut failures) = (0, Vec::new());
    for line in reader.lines() {
        let case: Value = serde_json::from_str(&line.unwrap()).unwrap();
        let encoding = Encoding::lookup(case["encoding"].as_str().unwrap().as_bytes()).unwrap();
        let input = bytes(case["input"].as_str().unwrap());
        let actual = mime::decode_header(&input, encoding);
        let expected = bytes(case["output"].as_str().unwrap());
        if actual != expected {
            if failures.len() < 12 { eprintln!("{case}\nactual={actual:?}\nexpected={expected:?}"); }
            failures.push(count);
        }
        count += 1;
    }
    assert!(count > 80_000, "incomplete MIME fixture: {count}");
    assert!(failures.is_empty(), "{} of {count} MIME cases differ, first {:?}", failures.len(), &failures[..failures.len().min(20)]);
}

/// Checks the encoding algorithm against every successful PHP request before ABI coercion tests.
#[test]
fn mime_encode_matches_php() {
    let fixture = include_bytes!("fixtures/mime_encode.jsonl.gz");
    let reader = BufReader::new(GzDecoder::new(fixture.as_slice()));
    let (mut count, mut failures) = (0, Vec::new());
    for line in reader.lines() {
        let case: Value = serde_json::from_str(&line.unwrap()).unwrap();
        if case.get("error").is_some() { continue; }
        let source = Encoding::lookup(case["internal"].as_str().unwrap().as_bytes()).unwrap();
        let language = elephc_mbstring::state::Language::lookup(case["language"].as_str().unwrap().as_bytes()).unwrap();
        let mail = language.mail_encodings();
        let options = case["options"].as_array().unwrap();
        let destination = options.first().map(|value| Encoding::lookup_c_string(&bytes(value["bytes"].as_str().unwrap())).unwrap()).unwrap_or(mail[0]);
        let base64 = (options.first().is_some() || !mail[1].name().starts_with(['Q', 'q']))
            && !options.get(1).and_then(|value| value.get("bytes")).is_some_and(|value| bytes(value.as_str().unwrap()).first().is_some_and(|byte| matches!(byte, b'Q' | b'q')));
        let separator = options.get(2).map(|value| bytes(value["bytes"].as_str().unwrap())).unwrap_or(b"\r\n".to_vec());
        let indent = options.get(3).and_then(Value::as_i64).unwrap_or(0);
        let actual = mime::encode_header(&bytes(case["input"].as_str().unwrap()), source, destination, base64, &separator, indent);
        let expected = bytes(case["output"].as_str().unwrap());
        if actual != expected {
            if failures.len() < 12 { eprintln!("{case}\nactual={actual:?}\nexpected={expected:?}"); }
            failures.push(count);
        }
        count += 1;
    }
    assert!(count > 65_000, "incomplete successful MIME encoder fixture: {count}");
    assert!(failures.is_empty(), "{} of {count} MIME encoder cases differ, first {:?}", failures.len(), &failures[..failures.len().min(20)]);
}
