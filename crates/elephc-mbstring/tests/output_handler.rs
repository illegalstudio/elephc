//! Purpose:
//! Verifies output-handler phases, header planning, and incremental codec behavior.
//!
//! Called from:
//! - The focused shared mbstring integration-test harness.
//!
//! Key details:
//! - PHP 8.5.10 fixtures isolate every sequence in a fresh request.
//! - Output bytes and illegal-character deltas are independent oracle observations.

use std::io::{BufRead, BufReader};
use elephc_mbstring::state::{OutputHeaders, State};
use flate2::read::GzDecoder;
use serde_json::Value;

/// Restores one binary fixture field from hexadecimal text.
fn bytes(value: &Value) -> Vec<u8> {
    let hex = value.as_str().unwrap();
    (0..hex.len()).step_by(2).map(|offset| u8::from_str_radix(&hex[offset..offset + 2], 16).unwrap()).collect()
}

/// Supplies the default convertible response metadata used by the isolated CLI oracle.
fn default_headers() -> OutputHeaders<'static> {
    OutputHeaders { mimetype: None, default_mimetype: None, send_default_content_type: true, in_handler: false }
}

/// Applies fixture-selected request settings without altering the active output decoder state.
fn configure(state: &mut State, values: &Value) {
    if let Some(name) = values["from"].as_str() { state.set_internal_encoding(name.as_bytes()).unwrap(); }
    if let Some(name) = values["to"].as_str() { state.set_http_output(name.as_bytes()).unwrap(); }
    if let Some(code) = values["substitute"].as_i64() { state.set_substitute_codepoint(code).unwrap(); }
    else if let Some(mode) = values["substitute"].as_str() { state.set_substitute_mode(mode.as_bytes()).unwrap(); }
}

/// Compares source carry, destination flush policy, and substitutions across all catalog encodings.
#[test]
fn output_handler_codec_feeds_match_php() {
    let fixture = include_bytes!("fixtures/output_handler.jsonl.gz");
    let reader = BufReader::new(GzDecoder::new(fixture.as_slice()));
    let mut failures = Vec::new();
    let mut count = 0;
    for line in reader.lines() {
        let case: Value = serde_json::from_str(&line.unwrap()).unwrap();
        let mut state = State::default();
        configure(&mut state, &case);
        for (index, step) in case["steps"].as_array().unwrap().iter().enumerate() {
            configure(&mut state, &case["changes"][index]);
            let plan = state.prepare_output(step["phase"].as_i64().unwrap(), default_headers(), false);
            let before = state.illegal_chars();
            let actual = state.output_chunk(&bytes(&step["input"]), plan);
            let errors = state.illegal_chars().wrapping_sub(before);
            if actual != bytes(&step["output"]) || errors != step["errors"].as_u64().unwrap() {
                failures.push(format!("{} -> {}, substitute={}, step={index}: bytes {} vs {}, errors {errors} vs {}",
                    case["from"], case["to"], case["substitute"], actual.len(), bytes(&step["output"]).len(), step["errors"]));
            }
        }
        count += 1;
    }
    assert_eq!(count, 264, "incomplete output fixture");
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Plans charset replacement only for START and suppresses direct header writes inside a handler.
#[test]
fn output_handler_header_plan_preserves_phase_policy() {
    let mut state = State::default();
    state.set_http_output(b"ISO-8859-1").unwrap();
    assert_eq!(state.prepare_output(1, default_headers(), false).header,
        Some(b"Content-Type: text/html; charset=ISO-8859-1".to_vec()));
    assert!(state.prepare_output(0, default_headers(), false).header.is_none());
    let headers = OutputHeaders { mimetype: Some(b"application/xhtml+xml; charset=UTF-8"),
        default_mimetype: None, send_default_content_type: false, in_handler: false };
    assert_eq!(state.prepare_output(1, headers, true).header,
        Some(b"Content-Type: application/xhtml+xml; charset=ISO-8859-1".to_vec()));
    let headers = OutputHeaders { in_handler: true, ..default_headers() };
    let plan = state.prepare_output(9, headers, false);
    assert!(plan.header.is_none());
    assert_eq!(state.output_chunk("Café".as_bytes(), plan), b"Caf\xe9");
}

/// Leaves conversion disabled for unmatched explicit MIME types and retains prior activation on START.
#[test]
fn output_handler_unmatched_mime_does_not_reset_an_active_feed() {
    let mut state = State::default();
    state.set_http_output(b"ISO-8859-1").unwrap();
    let headers = || OutputHeaders { mimetype: Some(b"image/png"), default_mimetype: None,
        send_default_content_type: false, in_handler: false };
    let plan = state.prepare_output(1, headers(), false);
    assert!(plan.header.is_none());
    assert_eq!(state.output_chunk("é".as_bytes(), plan), "é".as_bytes());
    let plan = state.prepare_output(1, default_headers(), false);
    assert_eq!(state.output_chunk("é".as_bytes(), plan), b"\xe9");
    let plan = state.prepare_output(1, headers(), false);
    assert_eq!(state.output_chunk("é".as_bytes(), plan), b"\xe9");
    let plan = state.prepare_output(8, headers(), false);
    assert_eq!(state.output_chunk(b"", plan), b"");
    let plan = state.prepare_output(0, headers(), false);
    assert_eq!(state.output_chunk("é".as_bytes(), plan), "é".as_bytes());
}

/// Reads live source settings after header publication while preserving the captured destination.
#[test]
fn output_handler_plan_keeps_destination_across_header_publication() {
    let mut state = State::default();
    state.set_http_output(b"ISO-8859-1").unwrap();
    let plan = state.prepare_output(9, default_headers(), false);
    state.set_http_output(b"ASCII").unwrap();
    state.set_internal_encoding(b"UTF-16LE").unwrap();
    assert_eq!(state.output_chunk(b"\xe9\0", plan), b"\xe9");
}
