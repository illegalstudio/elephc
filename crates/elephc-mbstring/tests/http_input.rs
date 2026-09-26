//! Purpose:
//! Verifies HTTP input selectors, configured names, and independent source identifications.
//!
//! Called from:
//! - The focused mbstring engine integration harness.
//!
//! Key details:
//! - The PHP fixture includes every single-byte selector and exact weak parser diagnostics.
//! - Parser adapters own when aggregate and source-specific identifications are recorded.

use std::io::{BufRead, BufReader};
use elephc_builtin_contract::{RuntimeBuiltinId, mbstring_abi::*};
use elephc_mbstring::{abi::*, coercion::{self, Input, Prepared}, encoding::{Encoding, EncodingList}, error::MbError,
    state::{Information, InputInformation, InputSource, OutputEncoding, State}};
use flate2::read::GzDecoder;
use serde_json::{json, Value};

/// Restores a binary selector from the independent PHP fixture.
fn bytes(value: &Value) -> Vec<u8> {
    let hex = value["bytes"].as_str().unwrap();
    (0..hex.len()).step_by(2).map(|index| u8::from_str_radix(&hex[index..index + 2], 16).unwrap()).collect()
}

/// Serializes strings without changing invalid UTF-8 or embedded NUL bytes.
fn string(value: &[u8]) -> Value { json!({"bytes": value.iter().map(|byte| format!("{byte:02x}")).collect::<String>()}) }

/// Resolves explicit host configuration names independently of selector lookup.
fn encoding(name: &str) -> OutputEncoding {
    if name == "pass" { OutputEncoding::Pass } else { OutputEncoding::Convert(Encoding::lookup(name.as_bytes()).unwrap()) }
}

/// Compares all selector bytes, list ordering, and weak argument errors with captured PHP calls.
#[test]
fn http_input_matches_php() {
    let reader = BufReader::new(GzDecoder::new(include_bytes!("fixtures/http_input.jsonl.gz").as_slice()));
    let mut count = 0;
    for line in reader.lines() {
        let case: Value = serde_json::from_str(&line.unwrap()).unwrap();
        let mut state = State::default();
        let encodings: Vec<_> = case["encodings"].as_array().unwrap().iter().map(|name| encoding(name.as_str().unwrap())).collect();
        state.set_http_input_encodings(&encodings);
        state.set_internal_encoding(b"UTF-16LE").unwrap();
        state.set_detect_order(EncodingList::CommaSeparated(b"ASCII")).unwrap();
        state.set_language(b"Japanese").unwrap();
        let args = case["args"].as_array().unwrap();
        if let Some(message) = coercion::arity_error(RuntimeBuiltinId::MbHttpInput, args.len()) {
            assert_eq!(case["error"], json!(["ArgumentCountError", String::from_utf8(message).unwrap()]));
            count += 1;
            continue;
        }
        let mut warnings = Vec::new();
        let selector = if args.is_empty() { None } else {
            let value = &args[0];
            let storage = value.get("bytes").map(|_| bytes(value));
            let input = if let Some(bytes) = &storage { Input::String(bytes) }
                else if value.is_null() { Input::Null }
                else if let Some(value) = value.as_bool() { Input::Bool(value) }
                else if let Some(value) = value.as_i64() { Input::Int(value) }
                else if let Some(value) = value.as_f64() { Input::Float(value.to_bits()) }
                else { Input::Array };
            let plan = coercion::prepare(RuntimeBuiltinId::MbHttpInput, 0, input, false).unwrap();
            warnings.extend(plan.diagnostics.iter().map(|warning| json!([warning.level, String::from_utf8(warning.message.clone()).unwrap()])));
            match plan.value {
                Ok(Prepared::Null) => None,
                Ok(Prepared::String(value)) => Some(value.into_owned()),
                Ok(Prepared::FormatFloat(bits)) => Some(f64::from_bits(bits).to_string().into_bytes()),
                Err(error) => {
                    assert_eq!(case["error"], json!(["TypeError", String::from_utf8(error).unwrap()]), "{case}");
                    assert_eq!(case["warnings"], json!(warnings), "{case}");
                    count += 1;
                    continue;
                },
                other => panic!("unexpected HTTP input argument {other:?}"),
            }
        };
        match state.http_input(selector.as_deref()) {
            Ok(result) => {
                let output = match result {
                    InputInformation::Unidentified => json!(false), InputInformation::String(bytes) => string(&bytes),
                    InputInformation::List(values) => json!({"array": values.iter().map(|value| string(value)).collect::<Vec<_>>()}),
                };
                assert_eq!(case["output"], output, "{case}");
            },
            Err(MbError::Value(message)) => assert_eq!(case["error"], json!(["ValueError", message]), "{case}"),
            Err(error) => panic!("unexpected HTTP input error {error:?}"),
        }
        assert_eq!(case["warnings"], json!(warnings), "{case}");
        count += 1;
    }
    assert_eq!(count, 2176);
}

/// Retains each source, aggregate result, and configured list independently across host updates.
#[test]
fn http_input_identification_is_independent() {
    let mut state = State::default();
    let sources = [(InputSource::Get, b"G", "SJIS"), (InputSource::Post, b"P", "UTF-8"),
        (InputSource::Cookie, b"C", "pass"), (InputSource::String, b"S", "ASCII")];
    for &(source, selector, name) in &sources {
        state.set_http_input_source_identification(source, Some(encoding(name)));
        assert_eq!(state.http_input(Some(selector)).unwrap(), InputInformation::String(name.as_bytes().to_vec()));
        assert_eq!(state.http_input(None).unwrap(), InputInformation::Unidentified);
    }
    state.set_http_input_identification(Some(encoding("8bit")));
    assert_eq!(state.http_input(None).unwrap(), InputInformation::String(b"8bit".to_vec()));
    assert_eq!(state.info(b"http_input"), Some(Information::String(b"8bit".to_vec())));
    for &(_, selector, name) in &sources {
        assert_eq!(state.http_input(Some(selector)).unwrap(), InputInformation::String(name.as_bytes().to_vec()));
    }
    state.set_http_input_source_identification(InputSource::Get, None);
    assert_eq!(state.http_input(Some(b"G")).unwrap(), InputInformation::Unidentified);
    state.set_http_input_encodings(&[]);
    assert_eq!(state.http_input(Some(b"I")).unwrap(), InputInformation::List(vec![]));
    assert_eq!(state.http_input(Some(b"L")).unwrap(), InputInformation::Unidentified);
    assert_eq!(state.http_input(None).unwrap(), InputInformation::String(b"8bit".to_vec()));
}

/// Preserves omitted/null getters and returns an owned indexed list through the actual wire.
#[test]
fn http_input_abi_defaults() {
    elephc_mbstring_reset_v1();
    for arguments in [vec![], vec![MbArgV1::null()], vec![MbArgV1::string(b"I")], vec![MbArgV1::string(b"L")]] {
        let mut result = MbResultV1::default();
        unsafe { elephc_mbstring_call_v1(RuntimeBuiltinId::MbHttpInput.as_u32(), arguments.as_ptr(), arguments.len() as u64, &mut result); }
        if arguments.is_empty() || arguments[0].kind == ARG_NULL {
            assert_eq!((result.kind, result.value), (RESULT_BOOL, 0));
        } else {
            let bytes = if result.len == 0 { &[] } else { unsafe { std::slice::from_raw_parts(result.bytes, result.len as usize) } };
            if result.kind == RESULT_STRING_ARRAY { assert_eq!(decode_string_array(bytes, result.value as usize), Some(vec![b"UTF-8".as_slice()])); }
            else { assert_eq!(result.kind, RESULT_STRING); assert_eq!(bytes, b"UTF-8"); }
        }
        assert_eq!(result.diagnostics_len, 0);
        unsafe { elephc_mbstring_release_v1(&mut result); }
    }
}
