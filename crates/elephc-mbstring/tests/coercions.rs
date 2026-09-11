//! Purpose:
//! Checks shared parameter-coercion decisions against independently captured PHP calls.
//!
//! Called from:
//! - Cargo's focused mbstring integration test harness.
//!
//! Key details:
//! - Prepared values pass through the actual mbstring C dispatch after coercion.
//! - Host float formatting and Stringable execution remain explicit actions with fixture-provided results.
//! - Every diagnostic byte, exception class, callback count, and final PHP result is compared.

use std::{borrow::Cow, io::{BufRead, BufReader}};
use elephc_builtin_contract::{RuntimeBuiltinId, mbstring_abi::*};
use elephc_builtin_contract::mbstring_abi::array::{ArrayGraph, Key, Value};
use elephc_builtin_contract::mbstring_abi::{coercion::*, host::*};
use elephc_mbstring::{abi::*, coercion::{self, Input, Prepared}};
use flate2::read::GzDecoder;
use serde_json::{json, Value as Json};

/// Decodes fixture bytes without applying UTF-8 conversion or numeric-string normalization.
fn unhex(text: &str) -> Vec<u8> {
    text.as_bytes().chunks_exact(2).map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap()).collect()
}

/// Encodes arbitrary diagnostic/result bytes in the oracle's lossless representation.
fn hex(bytes: &[u8]) -> String { bytes.iter().map(|byte| format!("{byte:02x}")).collect() }

/// Copies a live C result buffer before its owner is released.
unsafe fn copy(bytes: *const u8, length: u64) -> Vec<u8> {
    if length == 0 { Vec::new() } else { unsafe { std::slice::from_raw_parts(bytes, length as usize).to_vec() } }
}

/// Exercises the real preparation C ABI and copies its outputs before releasing bridge ownership.
fn prepare_c(operation: RuntimeBuiltinId, index: usize, input: Input<'_>, strict: bool) -> coercion::Preparation<'_> {
    let mut descriptor = MbCoercionInputV1 { kind: HOST_NULL, value: 0, bytes: std::ptr::null(), len: 0, flags: 0 };
    match input {
        Input::Null => {},
        Input::Bool(value) => { descriptor.kind = HOST_BOOL; descriptor.value = value as u64; },
        Input::Int(value) => { descriptor.kind = HOST_INT; descriptor.value = value as u64; },
        Input::Float(bits) => { descriptor.kind = HOST_FLOAT; descriptor.value = bits; },
        Input::String(bytes) => { descriptor.kind = HOST_STRING; descriptor.bytes = bytes.as_ptr(); descriptor.len = bytes.len() as u64; },
        Input::Array => { descriptor.kind = HOST_ASSOC_ARRAY; descriptor.value = u64::MAX; },
        Input::Object { class, stringable } => {
            descriptor.kind = INPUT_OBJECT; descriptor.value = u64::MAX; descriptor.bytes = class.as_ptr();
            descriptor.len = class.len() as u64; descriptor.flags = if stringable { INPUT_STRINGABLE } else { 0 };
        }
        Input::Resource { closed } => { descriptor.kind = INPUT_RESOURCE; descriptor.flags = if closed { INPUT_CLOSED_RESOURCE } else { 0 }; },
    }
    let mut output = MbResultV1::default();
    unsafe { elephc_mbstring_prepare_v1(operation.as_u32(), index as u32, &descriptor, strict as u32, &mut output); }
    let bytes = unsafe { copy(output.bytes, output.len) };
    let diagnostics = unsafe { copy(output.diagnostics, output.diagnostics_len) };
    let value = match output.kind {
        RESULT_TYPE_ERROR => Err(bytes),
        RESULT_INT => Ok(Prepared::Int(output.value)),
        RESULT_BOOL => Ok(Prepared::Bool(output.value != 0)),
        RESULT_STRING => Ok(Prepared::String(Cow::Owned(bytes))),
        PREPARED_NULL => Ok(Prepared::Null),
        PREPARED_ARRAY => Ok(Prepared::Array),
        PREPARED_FLOAT_STRING => Ok(Prepared::FormatFloat(output.value as u64)),
        PREPARED_STRINGABLE => Ok(Prepared::InvokeStringable),
        PREPARED_CALLABLE => Ok(Prepared::ResolveCallable),
        PREPARED_BORROWED_STRING => match input {
            Input::String(bytes) => Ok(Prepared::String(Cow::Borrowed(bytes))),
            _ => panic!("borrowed preparation must refer to the original string"),
        },
        kind => panic!("unexpected preparation result kind {kind}"),
    };
    unsafe { elephc_mbstring_release_v1(&mut output); }
    unsafe { elephc_mbstring_release_v1(&mut output); }
    let diagnostics = decode_diagnostics(&diagnostics).expect("complete diagnostic framing").into_iter()
        .map(|(level, message)| coercion::Diagnostic { level, message: message.to_vec() }).collect();
    coercion::Preparation { value, diagnostics }
}

/// Executes the real engine with one already prepared argument and normalizes its owned output.
fn execute(operation: RuntimeBuiltinId, index: usize, slot: MbArgV1) -> (Json, Vec<Json>) {
    let mut args = match operation {
        RuntimeBuiltinId::MbSubstr => if index == 0 {
            vec![slot, MbArgV1::integer(0), MbArgV1::null(), MbArgV1::string(b"8bit")]
        } else {
            vec![MbArgV1::string(b"abcdef"), MbArgV1::integer(if index == 2 { 1 } else { 0 }),
                MbArgV1::integer(1), MbArgV1::string(b"8bit")]
        },
        RuntimeBuiltinId::MbStrstr => vec![MbArgV1::string(b"abcd"), MbArgV1::string(b"b"), MbArgV1::boolean(false), MbArgV1::string(b"8bit")],
        RuntimeBuiltinId::MbCheckEncoding => vec![slot, MbArgV1::string(b"UTF-8")],
        RuntimeBuiltinId::MbSubstituteCharacter | RuntimeBuiltinId::MbInternalEncoding
        | RuntimeBuiltinId::MbDecodeMimeheader => vec![slot],
        _ => panic!("unexpected coercion fixture operation"),
    };
    args[index] = slot;
    let mut output = MbResultV1::default();
    unsafe { elephc_mbstring_call_v1(operation.as_u32(), args.as_ptr(), args.len() as u64, &mut output); }
    let bytes = unsafe { copy(output.bytes, output.len) };
    let diagnostics = unsafe { copy(output.diagnostics, output.diagnostics_len) };
    let value = match output.kind {
        RESULT_STRING => json!(["string", hex(&bytes)]),
        RESULT_INT => json!(["int", output.value]),
        RESULT_BOOL => json!(["bool", output.value != 0]),
        RESULT_TYPE_ERROR => json!(["error", "TypeError", hex(&bytes)]),
        RESULT_VALUE_ERROR => json!(["error", "ValueError", hex(&bytes)]),
        kind => panic!("unexpected C result kind {kind}: {}", String::from_utf8_lossy(&bytes)),
    };
    unsafe { elephc_mbstring_release_v1(&mut output); }
    let warnings = diagnostics.split(|&byte| byte == b'\n').filter(|line| !line.is_empty()).map(|line| {
        if let Some(message) = line.strip_prefix(b"Deprecated: ") { json!([8192, hex(message)]) }
        else if let Some(message) = line.strip_prefix(b"Warning: ") { json!([2, hex(message)]) }
        else { panic!("unexpected engine diagnostic"); }
    }).collect();
    (value, warnings)
}

/// Applies a pure coercion plan, simulating only the explicitly delegated host actions.
fn actual(case: &Json) -> (Json, Vec<Json>, Vec<Json>) {
    elephc_mbstring_reset_v1();
    let operation = RuntimeBuiltinId::MBSTRING.into_iter().find(|id| elephc_builtin_contract::lookup_id(id.builtin_id()).unwrap().name == case["function"].as_str().unwrap()).unwrap();
    let index = case["parameter"].as_u64().unwrap() as usize;
    let metadata = &case["input"];
    let bytes = unhex(metadata["bytes"].as_str().unwrap_or(""));
    let input = match metadata["kind"].as_str().unwrap() {
        "null" => Input::Null,
        "bool" => Input::Bool(metadata["value"].as_bool().unwrap()),
        "int" => Input::Int(metadata["value"].as_i64().unwrap()),
        "float" => Input::Float(u64::from_str_radix(metadata["bits"].as_str().unwrap(), 16).unwrap()),
        "string" => Input::String(&bytes),
        "array" => Input::Array,
        "object" => Input::Object { class: b"CoercionObject", stringable: false },
        "stringable" => Input::Object { class: b"CoercionText", stringable: true },
        "resource" => Input::Resource { closed: false },
        "closed-resource" => Input::Resource { closed: true },
        other => panic!("unexpected input descriptor {other}"),
    };
    let strict = case["strict"].as_bool().unwrap();
    let prepared = prepare_c(operation, index, input, strict);
    assert_eq!(prepared, coercion::prepare(operation, index, input, strict).unwrap());
    let mut warnings: Vec<_> = prepared.diagnostics.iter().map(|entry| json!([entry.level, hex(&entry.message)])).collect();
    let mut trace = Vec::new();
    let value = match prepared.value {
        Err(message) => return (json!(["error", "TypeError", hex(&message)]), warnings, trace),
        Ok(Prepared::InvokeStringable) => {
            trace.push(json!("stringify"));
            if metadata["throws"].as_bool().unwrap() {
                return (json!(["error", "RuntimeException", hex(b"coercion callback failed")]), warnings, trace);
            }
            Prepared::String(Cow::Borrowed(&bytes))
        }
        Ok(Prepared::FormatFloat(_)) => Prepared::String(Cow::Owned(unhex(metadata["formatted"].as_str().unwrap()))),
        Ok(value) => value,
    };
    let array = if metadata["kind"] == "array" {
        let string = if metadata["invalid"].as_bool().unwrap() { vec![255] } else { "猫é".as_bytes().to_vec() };
        ArrayGraph::new(0, vec![vec![(Key::String(b"name".to_vec()), Value::String(string))]]).unwrap().encode()
    } else { Vec::new() };
    let slot = match &value {
        Prepared::Null => MbArgV1::null(), Prepared::Bool(value) => MbArgV1::boolean(*value),
        Prepared::Int(value) => MbArgV1::integer(*value), Prepared::String(value) => MbArgV1::string(value),
        Prepared::Array => MbArgV1::array(&array), _ => unreachable!("host actions were materialized"),
    };
    let (result, more) = execute(operation, index, slot);
    warnings.extend(more);
    (result, warnings, trace)
}

/// Compares every captured PHP result, diagnostic sequence, and Stringable invocation decision.
#[test]
fn mbstring_coercions_match_php() {
    let input = GzDecoder::new(include_bytes!("fixtures/coercions.jsonl.gz").as_slice());
    let mut count = 0;
    for (index, line) in BufReader::new(input).lines().enumerate() {
        let case: Json = serde_json::from_str(&line.unwrap()).unwrap();
        let (result, warnings, trace) = actual(&case);
        assert_eq!(result, case["result"], "case {index}: {case}");
        assert_eq!(json!(warnings), case["warnings"], "case {index}: {case}");
        assert_eq!(json!(trace), case["trace"], "case {index}: {case}");
        let (substitution, _) = execute(RuntimeBuiltinId::MbSubstituteCharacter, 0, MbArgV1::null());
        let (internal, _) = execute(RuntimeBuiltinId::MbInternalEncoding, 0, MbArgV1::null());
        assert_eq!(substitution, case["substitution"], "case {index}: {case}");
        assert_eq!(internal, json!(["string", hex(case["internal"].as_str().unwrap().as_bytes())]), "case {index}: {case}");
        count += 1;
    }
    assert_eq!(count, 42_400);
}

/// Requires every published ordinary mbstring parameter to have an explicit coercion plan.
#[test]
fn mbstring_coercion_contract_coverage() {
    for operation in RuntimeBuiltinId::MBSTRING {
        let contract = elephc_builtin_contract::lookup_id(operation.builtin_id()).unwrap();
        for (index, parameter) in contract.params.iter().enumerate() {
            // Output references use the capture/query host and never undergo scalar preparation.
            if parameter.by_ref { continue; }
            assert!(coercion::parameter_mask(parameter.ty).is_some(), "{}: {}", contract.name, parameter.name);
            assert!(coercion::prepare(operation, index, Input::Null, true).is_some());
        }
        assert!(coercion::prepare(operation, contract.params.len(), Input::Null, false).is_none());
    }
    assert!(coercion::prepare(RuntimeBuiltinId::Abs, 0, Input::Int(1), false).is_none());
}
