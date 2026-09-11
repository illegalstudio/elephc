//! Purpose:
//! Checks JIS and ISO-2022-JP scalar output, shifted input, validation, and legacy cuts.
//!
//! Called from:
//! - `cargo test -p elephc-mbstring --test jis`.
//!
//! Key details:
//! - Every byte pair is tested under each encoding's initial modes against independent PHP hashes.
//! - Concrete fixtures retain malformed escapes and historical streaming-cut differences.

use std::io::{BufRead, BufReader};
use elephc_mbstring::{encoding::{Encoding, Substitute, SubstituteMode, UnicodeEncoding}, text};
use flate2::read::GzDecoder;
use serde_json::Value;
use sha2::{Digest, Sha256};

/// Decodes an original encoded byte string from the hexadecimal oracle representation.
fn bytes(hex: &str) -> Vec<u8> {
    (0..hex.len()).step_by(2).map(|index| u8::from_str_radix(&hex[index..index + 2], 16).unwrap()).collect()
}

/// Compares every captured legacy cut and malformed decoder input to its exact PHP output.
#[test]
fn jis_contexts_and_cuts_match_php() {
    let reader = BufReader::new(GzDecoder::new(include_bytes!("fixtures/jis.jsonl.gz").as_slice()));
    let substitute = Substitute { mode: SubstituteMode::Character, character: 0xfffd };
    for line in reader.lines() {
        let case: Value = serde_json::from_str(&line.unwrap()).unwrap();
        let encoding = Encoding::lookup(case["encoding"].as_str().unwrap().as_bytes()).unwrap();
        let input = bytes(case["input"].as_str().unwrap());
        if let Some(expected) = case["cut"].as_str() {
            let output = text::strcut(&input, case["from"].as_i64().unwrap(), case["budget"].as_i64(), encoding, substitute).unwrap();
            assert_eq!(output, bytes(expected), "cut {case}");
        } else {
            let decoded = encoding.decode(&input);
            assert_eq!(decoded.is_valid(), case["valid"].as_bool().unwrap(), "validity {case}");
            assert_eq!(decoded.points.len() as u64, case["length"].as_u64().unwrap(), "length {case}");
            assert_eq!(UnicodeEncoding::Ucs4Be.encode(&decoded.points, substitute), bytes(case["decoded"].as_str().unwrap()), "decode {case}");
            assert_eq!(encoding.encode(&decoded.points, substitute), bytes(case["scrub"].as_str().unwrap()), "scrub {case}");
        }
    }
}

/// Covers the complete scalar encoder and every two-byte input under all initial character modes.
#[test]
fn jis_shifted_pairs_and_scalar_encoders_match_php() {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/jis-hashes.json")).unwrap();
    for (name, expected) in fixture["encodings"].as_object().unwrap() {
        let encoding = Encoding::lookup(name.as_bytes()).unwrap();
        let substitute = Substitute { mode: SubstituteMode::None, ..Substitute::default() };
        let mut hash = Sha256::new();
        for code in 0..=0x10ffff {
            let encoded = encoding.encode(&[code], substitute);
            hash.update((encoded.len() as u32).to_le_bytes());
            hash.update(encoded);
        }
        assert_eq!(format!("{:x}", hash.finalize()), expected["encode"].as_str().unwrap(), "{name} encode");
        let substitute = Substitute { mode: SubstituteMode::Character, character: 0xfffd };
        for (prefix, expected) in expected["prefixes"].as_object().unwrap() {
            let prefix_bytes = bytes(prefix);
            let mut hash = Sha256::new();
            for pair in 0..=65535u32 {
                let mut input = prefix_bytes.clone();
                input.extend_from_slice(&[(pair >> 8) as u8, pair as u8]);
                input.extend_from_slice(b"\x1b(B");
                let decoded = encoding.decode(&input);
                hash.update([u8::from(decoded.is_valid())]);
                hash.update((decoded.points.len() as u32).to_le_bytes());
                for output in [UnicodeEncoding::Ucs4Be.encode(&decoded.points, substitute), encoding.encode(&decoded.points, substitute)] {
                    hash.update((output.len() as u32).to_le_bytes());
                    hash.update(output);
                }
            }
            assert_eq!(format!("{:x}", hash.finalize()), expected.as_str().unwrap(), "{name} prefix {prefix}");
        }
    }
}
