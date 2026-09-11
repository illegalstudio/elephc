//! Purpose:
//! Replays PHP's HTTP query decoding, conversion, registration, and parser diagnostics.
//!
//! Called from:
//! - `cargo test -p elephc-mbstring --test parse_str`.
//!
//! Key details:
//! - Independent PHP workers supply startup-only parser limits and display-error settings.
//! - This tests the shared engine; public AOT/eval output-reference integration is separate.

use std::io::{BufRead, BufReader};
use elephc_mbstring::{arrays::{ArrayGraph, Key, Value as ArrayValue}, encoding::Encoding,
    input::{Query, Variables}, state::{InputInformation, OutputEncoding, State}};
use flate2::read::GzDecoder;
use serde_json::{json, Value};

/// Restores binary bytes without requiring valid UTF-8 in query names or values.
fn bytes(hex: &str) -> Vec<u8> {
    (0..hex.len()).step_by(2).map(|offset| u8::from_str_radix(&hex[offset..offset + 2], 16).unwrap()).collect()
}

/// Serializes a binary byte range into the oracle's explicit hexadecimal string shape.
fn string(bytes: &[u8]) -> Value { json!({"bytes": bytes.iter().map(|byte| format!("{byte:02x}")).collect::<String>()}) }

/// Expands an acyclic query graph into ordered entries without losing numeric key identity.
fn array(graph: &ArrayGraph, index: usize) -> Value {
    let entries = graph.arrays()[index].iter().map(|(key, value)| {
        let key = match key { Key::Int(index) => json!(index), Key::String(bytes) => string(bytes) };
        let value = match value { ArrayValue::String(bytes) => string(bytes), ArrayValue::Array(index) => array(graph, *index), other => panic!("unexpected query value {other:?}") };
        json!([key, value])
    }).collect::<Vec<_>>();
    json!({"array": entries})
}

/// Compares every parser stage with PHP, including complete rejection and partial nested outputs.
#[test]
fn query_parsing_matches_php() {
    let fixture = include_bytes!("fixtures/parse_str.jsonl.gz");
    let reader = BufReader::new(GzDecoder::new(fixture.as_slice()));
    let mut count = 0;
    for line in reader.lines() {
        let case: Value = serde_json::from_str(&line.unwrap()).unwrap();
        let mut state = State::default();
        let to = Encoding::lookup(case["to"].as_str().unwrap().as_bytes()).unwrap();
        let candidates = case["encodings"].as_array().unwrap().iter().map(|name| match name.as_str().unwrap() {
            "pass" => OutputEncoding::Pass, name => OutputEncoding::Convert(Encoding::lookup(name.as_bytes()).unwrap()),
        }).collect::<Vec<_>>();
        state.set_http_input_encodings(&candidates);
        state.set_http_input_identification(Some(OutputEncoding::Convert(to)));
        state.record_illegal_chars(7);
        if let Some(code) = case["substitute"].as_i64() { state.set_substitute_codepoint(code).unwrap(); }
        else { state.set_substitute_mode(case["substitute"].as_str().unwrap().as_bytes()).unwrap(); }
        let mut warnings = Vec::new();
        let mut output = Variables::default();
        let identified = match Query::decode(&bytes(case["query"].as_str().unwrap()), &bytes(case["separator"].as_str().unwrap()), case["max_vars"].as_i64().unwrap()) {
            Err(error) => {
                warnings.push(json!([2, format!("mb_parse_str(): {}", error.message())]));
                None
            },
            Ok(query) => {
                let identified = query.identify(state.http_input_encodings(), case["strict"].as_bool().unwrap());
                if identified.warning { warnings.push(json!([2, "mb_parse_str(): Unable to detect encoding"])); }
                if let Some(from) = identified.encoding {
                    for pair in query.into_pairs() {
                        let pair = pair.convert(from, to, &mut state);
                        let maximum = case["max_nesting"].as_i64().unwrap();
                        if !output.register(&pair.name, pair.value, maximum) && !case["display_errors"].as_bool().unwrap() {
                            warnings.push(json!([2, format!("mb_parse_str(): Input variable nesting level exceeded {maximum}. To increase the limit change max_input_nesting_level in php.ini.")]));
                        }
                    }
                }
                identified.encoding
            },
        };
        state.set_http_input_identification(identified);
        let graph = output.into_graph();
        assert_eq!(array(&graph, graph.root()), case["output"], "output: {case}");
        assert_eq!(identified.is_some(), case["result"].as_bool().unwrap(), "result: {case}");
        assert_eq!(identified.map(|encoding| json!(encoding.name())).unwrap_or(json!(false)), case["identified"], "identity: {case}");
        assert_eq!(state.http_input(Some(b"S")).unwrap(), InputInformation::Unidentified);
        assert_eq!(state.illegal_chars() - 7, case["illegal"].as_u64().unwrap(), "conversion errors: {case}");
        assert_eq!(json!(warnings), case["warnings"], "diagnostics: {case}");
        count += 1;
    }
    assert_eq!(count, 17904, "incomplete query fixture");
}

/// Exposes partial output before a nesting warning and preserves later insertion ordering.
#[test]
fn query_nesting_overflow_removes_previous_root() {
    let mut output = Variables::default();
    assert!(output.register(b"a", b"old".to_vec(), 2));
    assert!(output.register(b"b", b"kept".to_vec(), 2));
    assert!(!output.register(b"a[x][y][z]", b"discarded".to_vec(), 2));
    assert_eq!(array(&output.snapshot(), 0), json!({"array": [[string(b"b"), string(b"kept")]]}));
    assert!(output.register(b"a[]", b"new".to_vec(), 2));
    assert_eq!(array(&output.into_graph(), 0), json!({"array": [[string(b"b"), string(b"kept")], [string(b"a"), {"array": [[0, string(b"new")]]}]]}));
}

/// Handles host-provided empty candidates and separators without conflating an empty raw query.
#[test]
fn query_empty_input_and_pass_configuration() {
    let query = Query::decode(b"a=1&b=2", b"", 1).unwrap();
    assert_eq!(query.identify(&[], true).encoding, Some(OutputEncoding::Pass));
    let pairs = query.into_pairs().collect::<Vec<_>>();
    assert_eq!(pairs[0].value, b"1&b=2");
    assert_eq!(Query::decode(b"\0ignored", b"&", 0).unwrap().identify(&[], false).encoding, None);
}

/// Stops a failed append before a later planned nesting removal can discard an existing root.
#[test]
fn query_append_failure_precedes_nesting_limit() {
    let mut output = Variables::default();
    assert!(output.register(b"a[9223372036854775807]", b"kept".to_vec(), 2));
    assert!(output.register(b"a[][z][q]", b"ignored".to_vec(), 2));
    assert_eq!(array(&output.into_graph(), 0), json!({"array": [[string(b"a"), {"array": [[i64::MAX, string(b"kept")]]}]]}));
}
