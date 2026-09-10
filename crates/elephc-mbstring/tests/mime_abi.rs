//! Purpose:
//! Verifies MIME encoder defaults, validation, and parameter parsing through the shared ABI.
//!
//! Called from:
//! - The focused mbstring integration test harness.
//!
//! Key details:
//! - Every captured PHP result includes empty-input errors and explicit-null coercion.
//! - Stateful charset deprecations have separate cache tests because fixture setup also converts inputs.

use std::{io::{BufRead, BufReader}, borrow::Cow};
use elephc_builtin_contract::{RuntimeBuiltinId, mbstring_abi::*};
use elephc_mbstring::{abi::*, coercion::{self, Input, Prepared}};
use flate2::read::GzDecoder;
use serde_json::Value;

/// Restores a lossless fixture string without decoding its byte payload.
fn bytes(hex: &str) -> Vec<u8> {
    (0..hex.len()).step_by(2).map(|index| u8::from_str_radix(&hex[index..index + 2], 16).unwrap()).collect()
}

/// Copies and releases all owned wire buffers before returning a test-friendly result.
fn call(operation: RuntimeBuiltinId, arguments: &[MbArgV1]) -> (u64, Vec<u8>, Vec<u8>) {
    let mut result = MbResultV1::default();
    unsafe { elephc_mbstring_call_v1(operation.as_u32(), arguments.as_ptr(), arguments.len() as u64, &mut result); }
    let value = if result.len == 0 { Vec::new() } else { unsafe { std::slice::from_raw_parts(result.bytes, result.len as usize).to_vec() } };
    let diagnostics = if result.diagnostics_len == 0 { Vec::new() }
        else { unsafe { std::slice::from_raw_parts(result.diagnostics, result.diagnostics_len as usize).to_vec() } };
    let kind = result.kind;
    unsafe { elephc_mbstring_release_v1(&mut result); }
    (kind, value, diagnostics)
}

/// Sets one request string option before the operation under test.
fn setting(operation: RuntimeBuiltinId, value: &[u8]) {
    assert_eq!(call(operation, &[MbArgV1::string(value)]).0, RESULT_BOOL);
}

/// Checks successful and failing captured requests after shared outer parameter coercion.
#[test]
fn mime_encode_abi_matches_php() {
    let fixture = include_bytes!("fixtures/mime_encode.jsonl.gz");
    let reader = BufReader::new(GzDecoder::new(fixture.as_slice()));
    let mut count = 0;
    for line in reader.lines() {
        let case: Value = serde_json::from_str(&line.unwrap()).unwrap();
        elephc_mbstring_reset_v1();
        setting(RuntimeBuiltinId::MbLanguage, case["language"].as_str().unwrap().as_bytes());
        setting(RuntimeBuiltinId::MbInternalEncoding, case["internal"].as_str().unwrap().as_bytes());
        if let Some(value) = case["substitute"].as_str() {
            setting(RuntimeBuiltinId::MbSubstituteCharacter, value.as_bytes());
        } else {
            assert_eq!(call(RuntimeBuiltinId::MbSubstituteCharacter, &[MbArgV1::integer(case["substitute"].as_i64().unwrap())]).0, RESULT_BOOL);
        }
        let input = bytes(case["input"].as_str().unwrap());
        let mut prepared = vec![Prepared::String(Cow::Owned(input))];
        let mut warnings = Vec::new();
        for (index, option) in case["options"].as_array().unwrap().iter().enumerate() {
            let storage = option.get("bytes").map(|value| bytes(value.as_str().unwrap()));
            let input = if let Some(storage) = &storage { Input::String(storage) }
                else if option.is_null() { Input::Null } else { Input::Int(option.as_i64().unwrap()) };
            let plan = coercion::prepare(RuntimeBuiltinId::MbEncodeMimeheader, index + 1, input, false).unwrap();
            warnings.extend(plan.diagnostics.into_iter().map(|warning| String::from_utf8(warning.message).unwrap()));
            prepared.push(match plan.value.unwrap() {
                Prepared::String(value) => Prepared::String(Cow::Owned(value.into_owned())),
                Prepared::Int(value) => Prepared::Int(value),
                other => panic!("unexpected MIME preparation {other:?}"),
            });
        }
        let arguments: Vec<_> = prepared.iter().map(|value| match value {
            Prepared::String(value) => MbArgV1::string(value), Prepared::Int(value) => MbArgV1::integer(*value),
            _ => unreachable!(),
        }).collect();
        let (kind, value, _) = call(RuntimeBuiltinId::MbEncodeMimeheader, &arguments);
        if let Some(error) = case.get("error") {
            assert_eq!((kind, value), (RESULT_VALUE_ERROR, error[1].as_str().unwrap().as_bytes().to_vec()), "{case}");
        } else {
            assert_eq!((kind, value), (RESULT_STRING, bytes(case["output"].as_str().unwrap())), "{case}");
        }
        let expected: Vec<_> = case["warnings"].as_array().unwrap().iter().filter_map(Value::as_str)
            .filter(|warning| warning.contains("Passing null")).collect();
        assert_eq!(warnings, expected, "{case}");
        count += 1;
    }
    assert!(count > 67_000);
}

/// Keeps reflected nullable declarations separate from PHP's nonnullable supplied-value parser.
#[test]
fn mime_encode_explicit_null_and_strict_types() {
    for (index, name) in [(1, "charset"), (2, "transfer_encoding")] {
        let weak = coercion::prepare(RuntimeBuiltinId::MbEncodeMimeheader, index, Input::Null, false).unwrap();
        assert_eq!(weak.value, Ok(Prepared::String(Cow::Borrowed(b""))));
        assert_eq!(weak.diagnostics[0].message, format!("mb_encode_mimeheader(): Passing null to parameter #{} (${name}) of type ?string is deprecated", index + 1).as_bytes());
        let strict = coercion::prepare(RuntimeBuiltinId::MbEncodeMimeheader, index, Input::Null, true).unwrap();
        assert!(strict.diagnostics.is_empty());
        assert_eq!(strict.value.unwrap_err(), format!("mb_encode_mimeheader(): Argument #{} (${name}) must be of type string, null given", index + 1).as_bytes());
    }
}

/// Preserves a deprecated forbidden charset in the shared last-name cache even after failure.
#[test]
fn mime_encode_charset_deprecation_cache() {
    elephc_mbstring_reset_v1();
    let args = [MbArgV1::string(b""), MbArgV1::string(b"Quoted-Printable")];
    let first = call(RuntimeBuiltinId::MbEncodeMimeheader, &args);
    assert_eq!(first.0, RESULT_VALUE_ERROR);
    assert!(first.2.starts_with(b"Deprecated: mb_encode_mimeheader(): "));
    let second = call(RuntimeBuiltinId::MbEncodeMimeheader, &args);
    assert_eq!(second.0, RESULT_VALUE_ERROR);
    assert!(second.2.is_empty());
}
