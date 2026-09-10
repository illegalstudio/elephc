//! Purpose:
//! Compares deprecated transfer encodings with PHP's complete conversion and text results.
//!
//! Called from:
//! - `cargo test -p elephc-mbstring --test transfer`.
//!
//! Key details:
//! - Fixtures distinguish raw fast conversion from character-oriented transformations.
//! - Malformed escapes, line limits, source overrides, and legacy cut flushes are retained.

use std::io::{BufRead, BufReader};
use elephc_mbstring::{encoding::{Encoding, Substitute, SubstituteMode}, text, unicode::CaseMode};
use flate2::read::GzDecoder;
use serde_json::Value;
use sha2::{Digest, Sha256};

/// Restores arbitrary encoded bytes from their lossless hexadecimal fixture form.
fn bytes(hex: &str) -> Vec<u8> {
    (0..hex.len()).step_by(2).map(|index| u8::from_str_radix(&hex[index..index + 2], 16).unwrap()).collect()
}

/// Checks complete transfer operations, including historical cut output and all substitute modes.
#[test]
fn transfer_operations_match_php() {
    let reader = BufReader::new(GzDecoder::new(include_bytes!("fixtures/transfer.jsonl.gz").as_slice()));
    for line in reader.lines() {
        let case: Value = serde_json::from_str(&line.unwrap()).unwrap();
        let encoding = Encoding::lookup(case["encoding"].as_str().unwrap().as_bytes()).unwrap();
        let input = bytes(case["input"].as_str().unwrap());
        let mode = match case["substitute"].as_str() {
            Some("none") => SubstituteMode::None,
            Some("long") => SubstituteMode::Long,
            Some("entity") => SubstituteMode::Entity,
            _ => SubstituteMode::Character,
        };
        let substitute = Substitute { mode, character: case["character"].as_u64().unwrap_or(0xfffd) as u32 };
        let outputs = match case["kind"].as_str().unwrap() {
            "convert" => {
                let to = Encoding::lookup(case["to"].as_str().unwrap().as_bytes()).unwrap();
                vec![("output", text::convert_encoding(&input, encoding, to, substitute))]
            }
            "cut" => vec![("output", text::strcut(&input, case["from"].as_i64().unwrap(), case["length"].as_i64(), encoding, substitute).unwrap())],
            _ => {
                assert_eq!(encoding.strlen(&input) as u64, case["length"].as_u64().unwrap(), "length {case}");
                assert_eq!(encoding.decode(&input).is_valid(), case["valid"].as_bool().unwrap(), "validity {case}");
                assert_eq!(text::strwidth(&input, encoding) as u64, case["width"].as_u64().unwrap(), "width {case}");
                vec![
                    ("scrub", text::scrub(&input, encoding, substitute)),
                    ("upper", text::convert_case(&input, CaseMode::Upper, encoding, substitute)),
                    ("lower", text::convert_case(&input, CaseMode::Lower, encoding, substitute)),
                    ("kana", text::convert_kana(&input, text::KanaMode::default(), encoding, substitute)),
                ]
            }
        };
        for (field, output) in outputs { assert_eq!(output, bytes(case[field].as_str().unwrap()), "{field} {case}"); }
    }
}

/// Hashes the full Unicode-range HTML encoder independently of its preferred-name table.
#[test]
fn html_entity_scalar_encoder_matches_php() {
    let manifest: Value = serde_json::from_str(include_str!("../src/encoding/transfer/html/manifest.json")).unwrap();
    let encoding = Encoding::lookup(b"HTML-ENTITIES").unwrap();
    let mut hash = Sha256::new();
    for code in 0..=0x10ffff {
        let output = encoding.encode(&[code], Substitute::default());
        hash.update((output.len() as u32).to_le_bytes());
        hash.update(output);
    }
    assert_eq!(format!("{:x}", hash.finalize()), manifest["encoder_hash"].as_str().unwrap());
}
