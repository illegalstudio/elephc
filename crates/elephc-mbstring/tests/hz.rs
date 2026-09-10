//! Purpose:
//! Verifies HZ shift transitions, all shifted byte pairs, encoder mappings, and streaming cuts.
//!
//! Called from:
//! - `cargo test -p elephc-mbstring --test hz`.
//!
//! Key details:
//! - Independent PHP fixtures protect the two mapping differences from EUC-CN.
//! - Tests include malformed pairs, line continuations, dangling shifts, and cut-flush budgets.

use std::io::{BufRead, BufReader};
use elephc_mbstring::{encoding::{Encoding, Substitute, SubstituteMode, UnicodeEncoding}, text};
use flate2::read::GzDecoder;
use serde_json::Value;
use sha2::{Digest, Sha256};

/// Decodes a hexadecimal fixture string without applying an implicit text encoding.
fn bytes(value: &Value) -> Vec<u8> {
    let hex = value.as_str().unwrap();
    (0..hex.len()).step_by(2).map(|index| u8::from_str_radix(&hex[index..index + 2], 16).unwrap()).collect()
}

/// Checks all recorded HZ decode/cut outputs and the full Unicode-range encoder hash.
#[test]
fn hz_shifts_cuts_and_mappings_match_php() {
    let encoding = Encoding::lookup(b"HZ").unwrap();
    let substitute = Substitute { mode: SubstituteMode::Character, character: 0xfffd };
    let reader = BufReader::new(GzDecoder::new(include_bytes!("fixtures/hz.jsonl.gz").as_slice()));
    for line in reader.lines() {
        let case: Value = serde_json::from_str(&line.unwrap()).unwrap();
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
    let expected: Value = serde_json::from_str(include_str!("fixtures/hz-encode.json")).unwrap();
    let mut hash = Sha256::new();
    for code in 0..=0x10ffff {
        let output = encoding.encode(&[code], substitute);
        hash.update((output.len() as u32).to_le_bytes());
        hash.update(output);
    }
    assert_eq!(format!("{:x}", hash.finalize()), expected["encode"].as_str().unwrap());
}
