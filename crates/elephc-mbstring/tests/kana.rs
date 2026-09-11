//! Purpose:
//! Verifies kana modes, validation, Unicode mappings, composition, and encoded output against PHP.
//!
//! Called from:
//! - `cargo test -p elephc-mbstring --test kana`.
//!
//! Key details:
//! - Every one-, two-, and three-flag combination is compared with an independent PHP oracle.
//! - Binary invalid flags and decoder boundaries retain exact output and exception bytes.

use std::io::{BufRead, BufReader};
use elephc_mbstring::{encoding::{Encoding, Substitute, SubstituteMode, UnicodeEncoding}, error::MbError, text::{self, KanaMode}};
use flate2::read::GzDecoder;
use serde_json::Value;
use sha2::{Digest, Sha256};

/// Restores original input or expected output bytes from the hexadecimal fixture format.
fn bytes(hex: &str) -> Vec<u8> {
    (0..hex.len()).step_by(2).map(|index| u8::from_str_radix(&hex[index..index + 2], 16).unwrap()).collect()
}

/// Compares complete operation results, ordered validation errors, and contextual conversions.
#[test]
fn kana_modes_and_encoding_operations_match_php() {
    let reader = BufReader::new(GzDecoder::new(include_bytes!("fixtures/kana.jsonl.gz").as_slice()));
    let substitute = Substitute { mode: SubstituteMode::Character, character: 0xfffd };
    for line in reader.lines() {
        let case: Value = serde_json::from_str(&line.unwrap()).unwrap();
        let mode = KanaMode::parse(&bytes(case["mode"].as_str().unwrap()));
        if let Some(expected) = case["error"].as_array() {
            let message = match mode.expect_err("PHP rejected these kana options") {
                MbError::Value(message) => message.into_bytes(),
                MbError::ValueBytes(message) => message,
                error => panic!("unexpected kana option error {error:?}"),
            };
            assert_eq!(expected[0], "ValueError");
            assert_eq!(message, bytes(expected[1].as_str().unwrap()), "error {case}");
        } else {
            let encoding = Encoding::lookup(case["encoding"].as_str().unwrap().as_bytes()).unwrap();
            let output = text::convert_kana(&bytes(case["input"].as_str().unwrap()), mode.unwrap(), encoding, substitute);
            assert_eq!(output, bytes(case["output"].as_str().unwrap()), "convert {case}");
        }
    }
}

/// Hashes every Unicode-range scalar for each flag to detect unintended transformation changes.
#[test]
fn kana_scalar_modes_match_php() {
    let oracle: Value = serde_json::from_str(include_str!("fixtures/kana-hashes.json")).unwrap();
    for (name, expected) in oracle["scalar_hashes"].as_object().unwrap() {
        let mode = KanaMode::parse(name.as_bytes()).unwrap();
        let mut hash = Sha256::new();
        for code in 0..=0x10ffff {
            let converted = mode.convert(&[code]);
            let output = UnicodeEncoding::Ucs4Be.encode(&converted, Substitute::default());
            hash.update((output.len() as u32).to_le_bytes());
            hash.update(output);
        }
        assert_eq!(format!("{:x}", hash.finalize()), expected.as_str().unwrap(), "{name}");
    }
}
