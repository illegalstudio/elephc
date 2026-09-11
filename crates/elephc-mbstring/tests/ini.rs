//! Purpose:
//! Checks raw INI storage, handler side effects, restore semantics, and request initialization.
//!
//! Called from:
//! - The focused mbstring engine integration harness.
//!
//! Key details:
//! - PHP traces compare ordered metadata and effective state after each successful or failed write.
//! - MIME compile results are supplied by the host; separate tests check that callback boundary.

use std::io::{BufRead, BufReader};
use elephc_mbstring::{arrays::{ArrayGraph, Key, Value as ArrayValue}, encoding::EncodingList,
    state::{CoreEncodingDefaults, Information, InputInformation, MimeRegexError, State}};
use flate2::read::GzDecoder;
use serde_json::{json, Value};

#[path = "ini/reentry.rs"]
mod reentry;

/// Decodes binary fixture text without imposing UTF-8 validity.
fn bytes(value: &Value) -> Vec<u8> {
    let hex = value["bytes"].as_str().unwrap();
    (0..hex.len()).step_by(2).map(|index| u8::from_str_radix(&hex[index..index + 2], 16).unwrap()).collect()
}

/// Encodes binary text in the independent PHP oracle format.
fn string(value: &[u8]) -> Value { json!({"bytes": value.iter().map(|byte| format!("{byte:02x}")).collect::<String>()}) }

/// Serializes an acyclic graph without losing key types or insertion order.
fn array(graph: &ArrayGraph, index: usize) -> Value {
    json!({"array": graph.arrays()[index].iter().map(|(key, value)| {
        let key = match key { Key::Int(value) => json!(value), Key::String(value) => string(value) };
        let value = match value {
            ArrayValue::Null => Value::Null, ArrayValue::Int(value) => json!(value),
            ArrayValue::String(value) => string(value), ArrayValue::Array(index) => array(graph, *index),
            other => panic!("unexpected INI value {other:?}"),
        };
        json!([key, value])
    }).collect::<Vec<_>>()})
}

/// Captures the same raw and effective state as the PHP fixture after a single operation.
fn snapshot(state: &State) -> Value {
    let Information::All(info) = state.info(b"all").unwrap() else { panic!("expected full information"); };
    let InputInformation::List(input) = state.http_input(Some(b"I")).unwrap() else { panic!("expected input list"); };
    let all = state.ini_get_all(true);
    let flat = state.ini_get_all(false);
    let (stack, retry) = state.ini_regex_limits();
    json!([array(&all, all.root()), array(&flat, flat.root()), array(&info, info.root()),
        {"array": input.iter().enumerate().map(|(index, value)| json!([index, string(value)])).collect::<Vec<_>>()},
        [stack, retry]])
}

/// Applies the fixture's public mutations without synthesizing raw INI values.
fn baseline(public: bool) -> State {
    let mut state = State::default();
    if public {
        state.set_language(b"Japanese").unwrap();
        state.set_internal_encoding(b"SJIS").unwrap();
        state.set_http_output(b"ISO-8859-1").unwrap();
        state.set_substitute_codepoint(35).unwrap();
        state.set_substitute_mode(b"entity").unwrap();
        state.set_detect_order(EncodingList::CommaSeparated(b"SJIS")).unwrap();
    }
    state
}

/// Reports the first differing leaf instead of printing several complete nested snapshots.
fn compare(actual: &Value, expected: &Value, path: &str, case: &str) {
    match (actual, expected) {
        (Value::Array(actual), Value::Array(expected)) => {
            assert_eq!(actual.len(), expected.len(), "{case}, array length at {path}");
            for (index, (actual, expected)) in actual.iter().zip(expected).enumerate() {
                compare(actual, expected, &format!("{path}[{index}]"), case);
            }
        },
        (Value::Object(actual), Value::Object(expected)) if actual.keys().eq(expected.keys()) => {
            for (key, value) in actual { compare(value, &expected[key], &format!("{path}.{key}"), case); }
        },
        _ => assert_eq!(actual, expected, "{case}, at {path}"),
    }
}

/// Replays PHP writes and repeated restores for binary keys, encodings, flags, substitutions, and quantities.
#[test]
fn ini_traces_match_php() {
    let reader = BufReader::new(GzDecoder::new(include_bytes!("fixtures/ini.jsonl.gz").as_slice()));
    let mut count = 0;
    for line in reader.lines() {
        let case: Value = serde_json::from_str(&line.unwrap()).unwrap();
        let mut state = baseline(case["public"].as_bool().unwrap());
        let name = bytes(&case["name"]);
        let value = bytes(&case["value"]);
        for (index, expected) in case["steps"].as_array().unwrap().iter().enumerate() {
            let result = if index == 0 { state.ini_set(&name, &value, |_| Ok(())) }
                else { state.ini_restore(&name, |_| Ok(())) };
            let output = if index != 0 { Value::Null }
                else { result.previous.as_deref().map_or(json!(false), string) };
            let actual = json!({"output": output, "warnings": result.diagnostics.iter()
                .map(|warning| json!([warning.level, string(&warning.message)])).collect::<Vec<_>>(), "state": snapshot(&state)});
            compare(&actual, expected, "result", &format!("case {count}, step {index}, name {name:?}, value {value:?}, public {}", case["public"]));
        }
        count += 1;
    }
    assert_eq!(count, 9446);
}

/// Checks validation sees trimmed C text and failed writes preserve raw storage until a successful restore.
#[test]
fn ini_mime_validation_and_restore() {
    let name = b"mbstring.http_output_conv_mimetypes";
    let mut state = State::default();
    let original = state.ini_get(name).unwrap().to_vec();
    let result = state.ini_set(name, b" \t[\0ignored\r\n", |pattern| {
        assert_eq!(pattern, b"[");
        Err(MimeRegexError { offset: 1, message: b"missing terminating ] for character class".to_vec() })
    });
    assert!(!result.accepted);
    assert_eq!(result.diagnostics[0].message, b"ini_set(): [ (offset=1): missing terminating ] for character class");
    assert_eq!(state.ini_get(name), Some(original.as_slice()));
    let mut calls = 0;
    assert!(state.ini_restore(name, |pattern| { calls += 1; assert_eq!(pattern, original); Ok(()) }).accepted);
    assert_eq!(calls, 1, "failed writes still mark an entry modified");
    assert!(state.ini_restore(name, |_| panic!("unmodified restore must not compile")).accepted);
    assert!(state.ini_set(name, b" \t\0\r\n", |_| panic!("trimmed empty pattern must not compile")).accepted);
    assert_eq!(state.http_output_conv_mimetypes(), b" \t\0\r\n");
}

/// Restores startup state and auto expansion without carrying input identification, counters, or live overrides.
#[test]
fn ini_startup_and_request_reset() {
    let settings: Vec<_> = [("mbstring.detect_order", "auto"), ("mbstring.language", "Korean"),
        ("mbstring.language", "Japanese"), ("mbstring.http_input", "ASCII,SJIS"),
        ("mbstring.strict_detection", "On"), ("mbstring.encoding_translation", "1")]
        .into_iter().map(|(key, value)| (key.as_bytes().to_vec(), value.as_bytes().to_vec())).collect();
    let defaults = CoreEncodingDefaults { internal: b"ISO-8859-1".to_vec(), input: b"UTF-8".to_vec(), output: b"UTF-16LE".to_vec() };
    let (mut state, warnings) = State::with_ini_configuration(&settings, defaults, |_| Ok(()));
    assert_eq!(warnings.len(), 1, "only explicit http_input is deprecated");
    assert_eq!(state.detect_order(), state.language().detect_order());
    assert_eq!(state.language().name(), "Japanese");
    assert_eq!(state.internal_encoding().name(), "ISO-8859-1");
    assert_eq!(state.http_output().name(), "UTF-16LE");
    let initial = snapshot(&state);
    state.set_language(b"Korean").unwrap();
    state.set_internal_encoding(b"SJIS").unwrap();
    state.ini_set(b"mbstring.http_input", b"pass", |_| Ok(()));
    state.record_illegal_chars(100);
    state.set_http_input_identification(Some(elephc_mbstring::state::OutputEncoding::Pass));
    state.reset_ini_request();
    assert_eq!(snapshot(&state), initial);
    state.ini_set(b"mbstring.detect_order", b"ASCII", |_| Ok(()));
    assert_eq!(state.detect_order(), state.language().detect_order(), "INI order changes apply on request startup");
}

/// Matches fresh PHP processes for explicit, inherited, invalid, and repeated startup assignments.
#[test]
fn ini_startup_matches_php() {
    let reader = BufReader::new(GzDecoder::new(include_bytes!("fixtures/ini_startup.jsonl.gz").as_slice()));
    let mut count = 0;
    for line in reader.lines() {
        let case: Value = serde_json::from_str(&line.unwrap()).unwrap();
        let overrides: Vec<_> = case["overrides"].as_array().unwrap().iter().map(|entry| (bytes(&entry[0]), bytes(&entry[1]))).collect();
        let defaults = CoreEncodingDefaults { internal: bytes(&case["defaults"][0]), input: bytes(&case["defaults"][1]), output: bytes(&case["defaults"][2]) };
        let (mut state, warnings) = State::with_ini_configuration(&overrides, defaults, |_| Ok(()));
        let label = format!("startup {count}, overrides {overrides:?}");
        compare(&snapshot(&state), &case["state"], "state", &label);
        compare(&json!(warnings.iter().map(|warning| json!([warning.level, string(&warning.message)])).collect::<Vec<_>>()), &case["warnings"], "warnings", &label);
        state.set_language(b"Russian").unwrap();
        state.set_internal_encoding(b"UTF-16LE").unwrap();
        state.record_illegal_chars(42);
        state.ini_set(b"mbstring.http_input", b"pass", |_| Ok(()));
        state.reset_ini_request();
        compare(&snapshot(&state), &case["state"], "reset", &label);
        count += 1;
    }
    assert_eq!(count, 141);
}

/// Honors explicit public and failed INI writes while updating all remaining inherited encodings.
#[test]
fn ini_core_encoding_inheritance() {
    let mut state = State::default();
    state.set_internal_encoding(b"SJIS").unwrap();
    assert!(!state.ini_set(b"mbstring.http_output", b"bogus", |_| Ok(())).accepted);
    let defaults = CoreEncodingDefaults { internal: b"UTF-16LE".to_vec(), input: b"ASCII,SJIS".to_vec(), output: b"ISO-8859-1".to_vec() };
    assert!(state.update_core_encoding_defaults(defaults).is_empty());
    assert_eq!(state.internal_encoding().name(), "SJIS");
    assert_eq!(state.http_output().name(), "UTF-8");
    assert_eq!(state.http_input(Some(b"L")).unwrap(), InputInformation::String(b"ASCII,SJIS".to_vec()));
    state.ini_restore(b"mbstring.http_output", |_| Ok(()));
    assert_eq!(state.http_output().name(), "ISO-8859-1");
    state.ini_restore(b"mbstring.internal_encoding", |_| Ok(()));
    assert_eq!(state.internal_encoding().name(), "SJIS", "public internal setter does not mark the INI entry modified");
    state.ini_set(b"mbstring.internal_encoding", b"", |_| Ok(()));
    assert_eq!(state.internal_encoding().name(), "UTF-16LE");
    state.reset_ini_request();
    assert_eq!(snapshot(&state), snapshot(&State::default()), "request-local core defaults must also reset");
}
