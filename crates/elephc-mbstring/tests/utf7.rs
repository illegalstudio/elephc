//! Purpose:
//! Checks UTF-7 code units, malformed shifts, scalar encoders, and streaming byte-budget cuts.
//!
//! Called from:
//! - `cargo test -p elephc-mbstring --test utf7`.
//!
//! Key details:
//! - Every BMP code unit is tested with and without a terminator, including surrogates.
//! - Encoder hashes include every Unicode-range value accepted from raw UCS input.

use std::io::{BufRead, BufReader};
use elephc_mbstring::{encoding::{Encoding, Substitute, SubstituteMode, UnicodeEncoding}, text};
use flate2::read::GzDecoder;
use serde_json::Value;
use sha2::{Digest, Sha256};

/// Recovers the original bytes from a hexadecimal PHP fixture field.
fn bytes(value: &Value) -> Vec<u8> {
    let hex = value.as_str().unwrap();
    (0..hex.len()).step_by(2).map(|index| u8::from_str_radix(&hex[index..index + 2], 16).unwrap()).collect()
}

/// Cross-checks conversion, validation, and legacy cuts, reporting the exact failing input.
#[test]
fn utf7_decoding_and_cuts_match_php() {
    let reader = BufReader::new(GzDecoder::new(include_bytes!("fixtures/utf7.jsonl.gz").as_slice()));
    let substitute = Substitute { mode: SubstituteMode::Character, character: 0xfffd };
    for line in reader.lines() {
        let case: Value = serde_json::from_str(&line.unwrap()).unwrap();
        let encoding = Encoding::lookup(case["encoding"].as_str().unwrap().as_bytes()).unwrap();
        let input = bytes(&case["input"]);
        if !case["cut"].is_null() {
            let cut = text::strcut(&input, case["from"].as_i64().unwrap(), case["budget"].as_i64(), encoding, substitute).unwrap();
            assert_eq!(cut, bytes(&case["cut"]), "cut {case}");
        } else {
            let decoded = encoding.decode(&input);
            assert_eq!(decoded.is_valid(), case["valid"].as_bool().unwrap(), "validity {case}");
            assert_eq!(decoded.points.len() as u64, case["length"].as_u64().unwrap(), "length {case}");
            assert_eq!(UnicodeEncoding::Ucs4Be.encode(&decoded.points, substitute), bytes(&case["decoded"]), "decode {case}");
            assert_eq!(encoding.encode(&decoded.points, substitute), bytes(&case["scrub"]), "scrub {case}");
        }
    }
}

/// Checks the entire UCS-to-UTF-7 encoder range, preserving PHP's surrogate encoding behavior.
#[test]
fn utf7_scalar_encoders_match_php() {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/utf7-encode.json")).unwrap();
    for (name, expected) in fixture["encodings"].as_object().unwrap() {
        let encoding = Encoding::lookup(name.as_bytes()).unwrap();
        let mut hash = Sha256::new();
        for code in 0..=0x10ffff {
            let output = encoding.encode(&[code], Substitute::default());
            hash.update((output.len() as u32).to_le_bytes());
            hash.update(output);
        }
        assert_eq!(format!("{:x}", hash.finalize()), expected.as_str().unwrap(), "{name}");
    }
}
