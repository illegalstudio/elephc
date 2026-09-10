//! Purpose:
//! Checks PHP information snapshots, selectors, diagnostics, and request state isolation.
//!
//! Called from:
//! - The focused mbstring engine integration harness.
//!
//! Key details:
//! - The PHP oracle retains insertion order, binary strings, and scalar type distinctions.
//! - Separate ABI checks exercise state mutations and successful null transport.

use std::io::{BufRead, BufReader};
use elephc_builtin_contract::{RuntimeBuiltinId, mbstring_abi::{array::{ArrayGraph, Key, Value as ArrayValue}, *}};
use elephc_mbstring::{abi::*, coercion::{self, Input, Prepared}, encoding::{Encoding, EncodingList},
    state::{Information, OutputEncoding, State}};
use flate2::read::GzDecoder;
use serde_json::{json, Value};

/// Decodes the oracle's binary string representation without UTF-8 assumptions.
fn bytes(value: &Value) -> Vec<u8> {
    let hex = value["bytes"].as_str().unwrap();
    (0..hex.len()).step_by(2).map(|index| u8::from_str_radix(&hex[index..index + 2], 16).unwrap()).collect()
}

/// Encodes one byte string in the oracle's lossless format.
fn string(value: &[u8]) -> Value { json!({"bytes": value.iter().map(|byte| format!("{byte:02x}")).collect::<String>()}) }

/// Encodes an acyclic information graph without losing array ordering or key identity.
fn array(graph: &ArrayGraph, index: usize) -> Value {
    json!({"array": graph.arrays()[index].iter().map(|(key, value)| {
        let key = match key { Key::Int(value) => json!(value), Key::String(value) => string(value) };
        let value = match value {
            ArrayValue::Null => Value::Null, ArrayValue::Int(value) => json!(value),
            ArrayValue::String(value) => string(value), ArrayValue::Array(index) => array(graph, *index),
            other => panic!("unexpected information value {other:?}"),
        };
        json!([key, value])
    }).collect::<Vec<_>>()})
}

/// Converts a successful information result into the independent PHP oracle representation.
fn information(value: Information) -> Value {
    match value {
        Information::Null => Value::Null, Information::Integer(value) => json!(value),
        Information::String(value) => string(&value),
        Information::Strings(values) => json!({"array": values.iter().enumerate()
            .map(|(index, value)| json!([index, string(value)])).collect::<Vec<_>>()}),
        Information::All(graph) => array(&graph, graph.root()),
    }
}

/// Applies only explicit fixture inputs, leaving observed output entirely to the engine.
fn configured(settings: &Value) -> State {
    let mut state = State::default();
    state.set_language(settings["language"].as_str().unwrap().as_bytes()).unwrap();
    state.set_internal_encoding(settings["internal"].as_str().unwrap().as_bytes()).unwrap();
    state.set_http_output(settings["output"].as_str().unwrap().as_bytes()).unwrap();
    let names: Vec<_> = settings["detect"].as_array().unwrap().iter().map(|value| value.as_str().unwrap().as_bytes().to_vec()).collect();
    state.set_detect_order(EncodingList::Array(&names)).unwrap();
    match &settings["substitute"] {
        Value::String(value) => state.set_substitute_mode(value.as_bytes()).unwrap(),
        value => state.set_substitute_codepoint(value.as_i64().unwrap()).unwrap(),
    }
    state.set_strict_detection(settings["strict"].as_bool().unwrap());
    state.set_http_output_conv_mimetypes(settings["mimetypes"].as_str().unwrap().as_bytes());
    state
}

/// Matches every captured selector, profile, parser failure, and ordered snapshot against PHP.
#[test]
fn info_matches_php() {
    let reader = BufReader::new(GzDecoder::new(include_bytes!("fixtures/info.jsonl.gz").as_slice()));
    let mut count = 0;
    for line in reader.lines() {
        let case: Value = serde_json::from_str(&line.unwrap()).unwrap();
        let state = configured(&case["settings"]);
        let args = case["args"].as_array().unwrap();
        if let Some(message) = coercion::arity_error(RuntimeBuiltinId::MbGetInfo, args.len()) {
            assert_eq!(case["error"], json!(["ArgumentCountError", String::from_utf8(message).unwrap()]));
            count += 1;
            continue;
        }
        let mut warnings = Vec::new();
        let selector = if args.is_empty() { b"all".to_vec() } else {
            let value = &args[0];
            let storage = value.get("bytes").map(|_| bytes(value));
            let input = if let Some(bytes) = &storage { Input::String(bytes) }
                else if value.is_null() { Input::Null }
                else if let Some(value) = value.as_bool() { Input::Bool(value) }
                else if let Some(value) = value.as_i64() { Input::Int(value) }
                else if let Some(value) = value.as_f64() { Input::Float(value.to_bits()) }
                else { Input::Array };
            let plan = coercion::prepare(RuntimeBuiltinId::MbGetInfo, 0, input, false).unwrap();
            warnings.extend(plan.diagnostics.iter().map(|warning| json!([warning.level, String::from_utf8(warning.message.clone()).unwrap()])));
            match plan.value {
                Ok(Prepared::String(value)) => value.into_owned(),
                Ok(Prepared::FormatFloat(bits)) => f64::from_bits(bits).to_string().into_bytes(),
                Err(error) => {
                    assert_eq!(case["error"], json!(["TypeError", String::from_utf8(error).unwrap()]), "{case}");
                    assert_eq!(case["warnings"], json!(warnings), "{case}");
                    count += 1;
                    continue;
                },
                other => panic!("unexpected information argument {other:?}"),
            }
        };
        let output = match state.info(&selector) {
            Some(value) => information(value),
            None => { warnings.push(json!([2, "mb_get_info(): argument #1 ($type) must be a valid type"])); json!(false) },
        };
        assert_eq!(case["output"], output, "{case}");
        assert_eq!(case["warnings"], json!(warnings), "{case}");
        count += 1;
    }
    assert_eq!(count, 5040);
}

/// Copies then releases the complete wire result, including its scalar payload and diagnostics.
fn call(operation: RuntimeBuiltinId, arguments: &[MbArgV1]) -> (u64, i64, Vec<u8>, Vec<u8>) {
    let mut result = MbResultV1::default();
    unsafe { elephc_mbstring_call_v1(operation.as_u32(), arguments.as_ptr(), arguments.len() as u64, &mut result); }
    let bytes = if result.len == 0 { Vec::new() } else { unsafe { std::slice::from_raw_parts(result.bytes, result.len as usize).to_vec() } };
    let diagnostics = if result.diagnostics_len == 0 { Vec::new() }
        else { unsafe { std::slice::from_raw_parts(result.diagnostics, result.diagnostics_len as usize).to_vec() } };
    let output = (result.kind, result.value, bytes, diagnostics);
    unsafe { elephc_mbstring_release_v1(&mut result); }
    output
}

/// Preserves null versus false, live conversion counts, and request reset through the real ABI.
#[test]
fn info_abi_state_and_reset() {
    use RuntimeBuiltinId::*;
    elephc_mbstring_reset_v1();
    assert_eq!(call(MbGetInfo, &[MbArgV1::string(b"http_input")]), (RESULT_NULL, 0, vec![], vec![]));
    let initial = call(MbGetInfo, &[]);
    assert_eq!(initial.0, RESULT_ARRAY);
    assert_eq!(initial, call(MbGetInfo, &[MbArgV1::string(b"ALL")]));
    assert_eq!(call(MbLanguage, &[MbArgV1::string(b"Japanese")]).0, RESULT_BOOL);
    assert_eq!(call(MbGetInfo, &[MbArgV1::string(b"mail_charset")]).2, b"ISO-2022-JP");
    assert_eq!(call(MbScrub, &[MbArgV1::string(b"\xff\xff"), MbArgV1::string(b"UTF-8")]).0, RESULT_STRING);
    assert_eq!(call(MbGetInfo, &[MbArgV1::string(b"illegal_chars")]), (RESULT_INT, 2, vec![], vec![]));
    assert_eq!(call(MbGetInfo, &[MbArgV1::string(b"all\0")]), (RESULT_BOOL, 0, vec![],
        b"Warning: mb_get_info(): argument #1 ($type) must be a valid type\n".to_vec()));
    elephc_mbstring_reset_v1();
    assert_eq!(call(MbGetInfo, &[]), initial);
}

/// Observes host configuration and input identification while preserving independent snapshots.
#[test]
fn info_host_state_and_snapshot_ownership() {
    let mut state = State::default();
    let initial = state.info(b"all").unwrap();
    state.set_encoding_translation(true);
    state.set_strict_detection(true);
    state.set_http_output_conv_mimetypes(b"^text/\0binary");
    state.set_http_input_identification(Some(OutputEncoding::Pass));
    state.record_illegal_chars(u64::MAX);
    assert_eq!(state.info(b"http_input"), Some(Information::String(b"pass".to_vec())));
    assert_eq!(state.info(b"encoding_translation"), Some(Information::String(b"On".to_vec())));
    assert_eq!(state.info(b"strict_detection"), Some(Information::String(b"On".to_vec())));
    assert_eq!(state.info(b"http_output_conv_mimetypes"), Some(Information::String(b"^text/\0binary".to_vec())));
    assert_eq!(state.info(b"illegal_chars"), Some(Information::Integer(-1)));
    let Information::All(graph) = state.info(b"all").unwrap() else { panic!("expected snapshot"); };
    assert_eq!(graph.arrays()[0][1], (Key::String(b"http_input".to_vec()), ArrayValue::String(b"pass".to_vec())));
    state.set_http_input_identification(Some(OutputEncoding::Convert(Encoding::lookup(b"SJIS").unwrap())));
    assert_eq!(state.info(b"http_input"), Some(Information::String(b"SJIS".to_vec())));
    assert_eq!(initial, State::default().info(b"all").unwrap());
}
