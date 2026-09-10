//! Purpose:
//! Compares recursive array validation and conversion with captured PHP results and diagnostics.
//!
//! Called from:
//! - Cargo's focused mbstring array integration test binary.
//!
//! Key details:
//! - Graph identities preserve cycles, aliases, insertion order, and integer/string key distinctions.
//! - Conversion checks include first-wins collisions and independent key/value detection.
//! - Nonterminating PHP recursion is represented as an explicit failure, never a fabricated array.

use std::io::{BufRead, BufReader};
use elephc_mbstring::{arrays::{self, ArrayGraph, ConversionFailure, Key, Value}, encoding::{Encoding, EncodingList}, state::State};
use flate2::read::GzDecoder;
use serde_json::{json, Value as Json};

/// Decodes one binary fixture string without assuming any source character encoding.
fn bytes(hex: &str) -> Vec<u8> {
    (0..hex.len()).step_by(2).map(|index| u8::from_str_radix(&hex[index..index + 2], 16).unwrap()).collect()
}

/// Serializes byte-exact output in the same lowercase form used by the PHP oracle.
fn hex(bytes: &[u8]) -> String { bytes.iter().map(|byte| format!("{byte:02x}")).collect() }

/// Restores one validated graph from the independent PHP descriptor fixture.
fn graph(json: &Json) -> ArrayGraph {
    let arrays = json.as_array().unwrap().iter().map(|entries| entries.as_array().unwrap().iter().map(|entry| {
        let key = if entry[0][0] == "i" { Key::Int(entry[0][1].as_i64().unwrap()) }
            else { Key::String(bytes(entry[0][1].as_str().unwrap())) };
        let value = &entry[1];
        let value = match value[0].as_str().unwrap() {
            "a" => Value::Array(value[1].as_u64().unwrap() as usize),
            "s" => Value::String(bytes(value[1].as_str().unwrap())),
            "i" => Value::Int(value[1].as_i64().unwrap()),
            "f" => Value::Float(u64::from_str_radix(value[1].as_str().unwrap(), 16).unwrap()),
            "b" => Value::Bool(value[1].as_bool().unwrap()),
            "n" => Value::Null,
            "o" | "r" => Value::Unsupported,
            other => panic!("unexpected fixture value {other}"),
        };
        (key, value)
    }).collect()).collect();
    ArrayGraph::new(0, arrays).expect("valid PHP input graph")
}

/// Normalizes an acyclic output value for comparison with PHP's complete result tree.
fn normalized(value: &Value, graph: &ArrayGraph) -> Json {
    match value {
        Value::Null => json!(["n"]),
        Value::Bool(value) => json!(["b", value]),
        Value::Int(value) => json!(["i", value]),
        Value::Float(value) => json!(["f", format!("{value:016x}")]),
        Value::String(value) => json!(["s", hex(value)]),
        Value::Unsupported => panic!("unsupported values cannot survive conversion"),
        Value::Array(index) => json!(["a", graph.arrays()[*index].iter().map(|(key, value)| {
            let key = match key { Key::Int(value) => json!(["i", value]), Key::String(value) => json!(["s", hex(value)]) };
            json!([key, normalized(value, graph)])
        }).collect::<Vec<_>>()]),
    }
}

/// Verifies every captured result, warning sequence, and illegal-character delta without running PHP.
#[test]
fn recursive_arrays_match_php() {
    let fixture = include_bytes!("fixtures/arrays.jsonl.gz");
    let reader = BufReader::new(GzDecoder::new(fixture.as_slice()));
    let mut count = 0;
    for line in reader.lines() {
        let case: Json = serde_json::from_str(&line.unwrap()).unwrap();
        let input = graph(&case["graph"]);
        let input = ArrayGraph::decode(&input.encode()).expect("input graph wire framing");
        let before = input.clone();
        let to = Encoding::lookup(case["to"].as_str().unwrap().as_bytes()).unwrap();
        let (result, warnings, errors) = if case["kind"] == "check" {
            let checked = arrays::check_encoding(&input, to);
            (json!(["b", checked.valid]), checked.warnings, 0)
        } else {
            let mut state = State::default();
            let list: Vec<Vec<u8>> = case["from"].as_array().map_or_else(Vec::new, |values|
                values.iter().map(|value| value.as_str().unwrap().as_bytes().to_vec()).collect());
            let list = match case["from"].as_str() { Some(value) => EncodingList::CommaSeparated(value.as_bytes()),
                None => EncodingList::Array(&list) };
            let sources = state.conversion_sources(Some(list)).unwrap();
            state.set_substitute_codepoint(case["substitute"].as_i64().unwrap_or(233)).unwrap();
            if let Some(mode) = case["substitute"].as_str() { state.set_substitute_mode(mode.as_bytes()).unwrap(); }
            state.set_strict_detection(case["strict"].as_bool().unwrap());
            let converted = state.convert_array(&input, to, &sources);
            assert_eq!(state.illegal_chars(), converted.illegal_chars);
            let output = converted.result.expect("finite PHP fixture must convert");
            let output = ArrayGraph::decode(&output.encode()).expect("converted graph wire framing");
            (normalized(&Value::Array(output.root()), &output), converted.warnings, converted.illegal_chars)
        };
        assert_eq!(result, case["result"], "result: {case}");
        assert_eq!(json!(warnings), case["warnings"], "warnings: {case}");
        assert_eq!(json!(errors), case["errors"], "errors: {case}");
        assert_eq!(input, before, "conversion mutated an input graph");
        count += 1;
    }
    assert_eq!(count, 3670);
}

/// Verifies dangling identities and duplicate keys cannot enter a graph traversal.
#[test]
fn array_graph_validation_rejects_invalid_metadata() {
    assert!(ArrayGraph::new(0, vec![]).is_none());
    assert!(ArrayGraph::new(1, vec![vec![]]).is_none());
    assert!(ArrayGraph::new(0, vec![vec![(Key::Int(0), Value::Array(1))]]).is_none());
    assert!(ArrayGraph::new(0, vec![vec![(Key::Int(0), Value::Null), (Key::Int(0), Value::Null)]]).is_none());
    assert!(ArrayGraph::new(0, vec![vec![(Key::Int(0), Value::Null), (Key::String(b"0".to_vec()), Value::Null)]]).is_some());
}

/// Verifies PHP's repeated self-reference failure is detected without recursive Rust calls.
#[test]
fn nonterminating_array_conversion_fails_explicitly() {
    let input = ArrayGraph::new(0, vec![vec![(Key::Int(0), Value::Array(0)), (Key::Int(1), Value::Array(0))]]).unwrap();
    let state = State::default();
    let sources = state.conversion_sources(None).unwrap();
    let converted = arrays::convert_encoding(&input, state.internal_encoding(), &sources, false, state.substitute());
    assert_eq!(converted.result, Err(ConversionFailure::NonTerminatingRecursion));
    assert_eq!(converted.warnings, vec!["mb_convert_encoding(): Cannot convert recursively referenced values"]);
    let checked = arrays::check_encoding(&input, state.internal_encoding());
    assert!(!checked.valid);
    assert_eq!(checked.warnings.len(), 2);
}

/// Verifies nested acyclic arrays use explicit work stacks and retain every leaf value.
#[test]
fn deeply_nested_arrays_preserve_structure() {
    let depth = 512;
    let mut nodes = (0..depth).map(|index| vec![(Key::Int(0), Value::Array(index + 1))]).collect::<Vec<_>>();
    nodes.push(vec![(Key::String(b"leaf".to_vec()), Value::String("é".as_bytes().to_vec()))]);
    let input = ArrayGraph::new(0, nodes).unwrap();
    let state = State::default();
    assert!(arrays::check_encoding(&input, state.internal_encoding()).valid);
    let sources = state.conversion_sources(None).unwrap();
    let converted = arrays::convert_encoding(&input, state.internal_encoding(), &sources, false, state.substitute());
    assert_eq!(converted.result, Ok(input));
    assert!(converted.warnings.is_empty());
    assert_eq!(converted.illegal_chars, 0);
}

/// Verifies visited strings still count when repeated recursion prevents a result from returning.
#[test]
fn nonterminating_array_conversion_retains_partial_error_count() {
    let input = ArrayGraph::new(0, vec![vec![
        (Key::Int(0), Value::String(vec![0xff])),
        (Key::Int(1), Value::Array(0)),
        (Key::Int(2), Value::Array(0)),
    ]]).unwrap();
    let mut state = State::default();
    let sources = state.conversion_sources(None).unwrap();
    let converted = state.convert_array(&input, state.internal_encoding(), &sources);
    assert_eq!(converted.result, Err(ConversionFailure::NonTerminatingRecursion));
    assert_eq!(converted.illegal_chars, 1);
    assert_eq!(state.illegal_chars(), 1);
    let ordinary = state.convert_string(b"\xff", state.internal_encoding(), &sources);
    assert_eq!(ordinary, Some(b"?".to_vec()));
    assert_eq!(state.illegal_chars(), 2);
}

/// Verifies discarded nested values still emit warnings and count errors after converted keys collide.
#[test]
fn colliding_array_values_are_converted_before_discard() {
    let input = ArrayGraph::new(0, vec![
        vec![(Key::String("é".as_bytes().to_vec()), Value::Array(1)), (Key::String("è".as_bytes().to_vec()), Value::Array(2))],
        vec![(Key::String(b"first".to_vec()), Value::String(b"a\xff".to_vec()))],
        vec![(Key::String(b"object".to_vec()), Value::Unsupported), (Key::String(b"last".to_vec()), Value::String(b"b\xff".to_vec()))],
    ]).unwrap();
    let mut state = State::default();
    let sources = state.conversion_sources(None).unwrap();
    let converted = state.convert_array(&input, Encoding::lookup(b"ASCII").unwrap(), &sources);
    let expected = ArrayGraph::new(0, vec![
        vec![(Key::String(b"?".to_vec()), Value::Array(1))],
        vec![(Key::String(b"first".to_vec()), Value::String(b"a?".to_vec()))],
    ]).unwrap();
    assert_eq!(converted.result, Ok(expected));
    assert_eq!(converted.warnings, vec!["mb_convert_encoding(): Object is not supported"]);
    assert_eq!(converted.illegal_chars, 4);
    assert_eq!(state.illegal_chars(), 4);
}
