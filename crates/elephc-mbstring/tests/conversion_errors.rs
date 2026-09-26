//! Purpose:
//! Compares counted conversions with PHP across every encoding pair and replacement mode.
//!
//! Called from:
//! - Cargo's focused mbstring conversion-errors integration test binary.
//!
//! Key details:
//! - The oracle records both byte-exact output and mb_get_info illegal-character deltas.
//! - Replacement failures count even when the fallback emits no bytes.

use std::io::{BufRead, BufReader};
use elephc_mbstring::{encoding::{Encoding, Substitute, SubstituteMode}, state::State, text};
use flate2::read::GzDecoder;
use serde_json::Value;

/// Restores a byte string from its independent hexadecimal fixture representation.
fn bytes(hex: &str) -> Vec<u8> {
    (0..hex.len()).step_by(2).map(|index| u8::from_str_radix(&hex[index..index + 2], 16).unwrap()).collect()
}

/// Verifies all source/destination pairs, malformed inputs, and recursively rejected replacements.
#[test]
fn conversion_error_counts_match_php() {
    let fixture = include_bytes!("fixtures/conversion-errors.jsonl.gz");
    let reader = BufReader::new(GzDecoder::new(fixture.as_slice()));
    let mut count = 0;
    for line in reader.lines() {
        let case: Value = serde_json::from_str(&line.unwrap()).unwrap();
        let from = Encoding::lookup(case["from"].as_str().unwrap().as_bytes()).unwrap();
        let to = Encoding::lookup(case["to"].as_str().unwrap().as_bytes()).unwrap();
        let mode = match case["substitute"].as_str() { Some("none") => SubstituteMode::None,
            Some("long") => SubstituteMode::Long, Some("entity") => SubstituteMode::Entity, _ => SubstituteMode::Character };
        let substitute = Substitute { mode, character: case["substitute"].as_u64().unwrap_or(233) as u32 };
        let actual = text::convert_encoding_with_errors(&bytes(case["input"].as_str().unwrap()), from, to, substitute);
        let expected = (bytes(case["output"].as_str().unwrap()), case["errors"].as_u64().unwrap());
        assert_eq!(actual, expected, "{case}");
        count += 1;
    }
    assert_eq!(count, 174748);
}

/// Verifies request counters include scrub errors and remain independent of other requests.
#[test]
fn scrub_updates_only_its_request_counter() {
    let utf8 = Encoding::lookup(b"UTF-8").unwrap();
    let ascii = Encoding::lookup(b"ASCII").unwrap();
    let mut state = State::default();
    state.set_substitute_codepoint(233).unwrap();
    assert_eq!(state.scrub(b"a\xff", ascii), b"a?");
    assert_eq!(state.illegal_chars(), 2);
    assert_eq!(state.scrub(b"\xff", utf8), "é".as_bytes());
    assert_eq!(state.illegal_chars(), 3);
    text::convert_encoding(b"\xff", utf8, ascii, Substitute::default());
    assert_eq!(state.illegal_chars(), 3);
    assert_eq!(State::default().illegal_chars(), 0);
}

/// Verifies failed detection does not count rejected candidates and successful calls accumulate.
#[test]
fn detected_string_conversion_counts_only_selected_codec() {
    use elephc_mbstring::encoding::EncodingList;
    let mut state = State::default();
    let ascii = Encoding::lookup(b"ASCII").unwrap();
    let sources = state.conversion_sources(Some(EncodingList::CommaSeparated(b"UTF-8,ASCII"))).unwrap();
    state.set_strict_detection(true);
    assert_eq!(state.convert_string(b"\xff", ascii, &sources), None);
    assert_eq!(state.illegal_chars(), 0);
    assert_eq!(state.convert_string("é".as_bytes(), ascii, &sources), Some(b"?".to_vec()));
    assert_eq!(state.illegal_chars(), 1);
    state.set_strict_detection(false);
    assert_eq!(state.convert_string(b"\xff", ascii, &sources), Some(b"?".to_vec()));
    assert_eq!(state.illegal_chars(), 2);
}
