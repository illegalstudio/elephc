//! Purpose:
//! Replays PHP progressive-regex traces through the public protected mbstring ABI.
//!
//! Called from:
//! - The focused invocation tests with the reviewed managed Oniguruma provider.
//!
//! Key details:
//! - Nested warning actions use actual public operations, not direct Session methods.
//! - Wire decoding checks binary captures, ordered keys, chained errors, and final request state.

use super::*;
use std::io::{BufRead, BufReader};
use elephc_builtin_contract::mbstring_abi::{array::{ArrayGraph, Key, Value as Cell},
    exception::{is_exception, MbExceptionV1}, ini::{MbIniHostV1, INI_SET}};

#[path = "regex_capture.rs"]
mod capture;

/// Retains the ordinary fixture host at offset zero for its unchanged value/ownership callbacks.
#[repr(C)]
struct Replay { host: Host, step: Value, warnings: Vec<String>, callbacks: Vec<Vec<Value>> }

/// Decodes one optional binary source value without conflating absent and empty strings.
fn bytes(value: &Value) -> Option<Vec<u8>> {
    value.as_str().map(|value| value.as_bytes().chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap()).collect())
}

/// Uses the independent PHP corpus's exact binary-string representation.
fn encoded(bytes: &[u8]) -> Value { json!({"bytes": hex(bytes)}) }

/// Copies a complete public result and consumes its bridge ownership exactly once.
fn decode_wire(mut output: MbResultV1) -> Result<Value, Value> {
    let bytes = if output.len == 0 { &[] } else { unsafe { std::slice::from_raw_parts(output.bytes, output.len as usize) } };
    let result = if is_exception(output.kind) {
        let mut previous = Value::Null;
        let mut record = MbExceptionV1 { kind: 0, bytes: std::ptr::null(), len: 0 };
        for index in 0.. {
            let status = unsafe { elephc_mbstring_exception_at_v1(&output, index, &mut record) };
            if status == 0 { break; }
            assert_eq!(status, 1);
            let class = match record.kind { RESULT_VALUE_ERROR => "ValueError", RESULT_TYPE_ERROR => "TypeError",
                RESULT_ERROR => "Error", _ => panic!("unexpected error class {}", record.kind) };
            let message = unsafe { std::slice::from_raw_parts(record.bytes, record.len as usize) };
            previous = json!([class, hex(message), previous]);
        }
        Err(previous)
    } else {
        Ok(match output.kind {
            RESULT_NULL => Value::Null,
            RESULT_BOOL => json!(output.value != 0), RESULT_INT => json!(output.value),
            RESULT_STRING | RESULT_INI_STRING => encoded(bytes),
            RESULT_ARRAY => {
                let graph = ArrayGraph::decode(bytes).expect("valid public result graph");
                json!({"array": graph.arrays()[graph.root()].iter().map(|(key, value)| {
                    let key = match key { Key::Int(key) => json!(key), Key::String(key) => encoded(key) };
                    let value = match value { Cell::Int(value) => json!(value), Cell::Bool(value) => json!(value),
                        Cell::String(value) => encoded(value), _ => panic!("unexpected capture cell {value:?}") };
                    json!([key, value])
                }).collect::<Vec<_>>()})
            },
            kind => panic!("unexpected public result {kind}"),
        })
    };
    unsafe { elephc_mbstring_release_v1(&mut output); }
    result
}

/// Runs an already typed request getter without borrowing any callback-owned value metadata.
fn getter(operation: RuntimeBuiltinId) -> Value {
    let mut output = MbResultV1::default();
    unsafe { elephc_mbstring_call_v1(operation.as_u32(), std::ptr::null(), 0, &mut output); }
    decode_wire(output).unwrap()
}

/// Returns a canonical live setting as text for comparison with the PHP trace envelope.
fn setting(operation: RuntimeBuiltinId) -> String {
    String::from_utf8(bytes(&getter(operation)["bytes"]).unwrap()).unwrap()
}

/// Reenters public operations from actual warning delivery and preserves handler-thrown exceptions.
unsafe extern "C" fn diagnostic(context: *mut c_void, level: u32, bytes: *const u8, len: u64) -> i32 {
    let replay = unsafe { &mut *context.cast::<Replay>() };
    if level != 2 { replay.host.errors.push(format!("unexpected diagnostic level {level}")); return 1; }
    let message = unsafe { std::slice::from_raw_parts(bytes, len as usize) };
    replay.warnings.push(hex(message));
    if replay.callbacks.is_empty() {
        if let Some(actions) = replay.step["on_warning"].as_array() {
            replay.callbacks.push(actions.iter().map(run).collect());
            if replay.step["throw"] == true {
                replay.host.pending = Some(json!(["RuntimeException", hex(b"regex handler failed"), null]));
                return 2;
            }
        }
    }
    0
}

/// Accepts the limit setters' diagnostic-free native INI contract.
unsafe extern "C" fn ini_diagnostic(_: *mut c_void, _: u32, _: *const u8, _: u64) -> i32 { 0 }

/// Updates live regex limits through the same INI ABI read by protected matching operations.
fn limit(step: &Value) -> Result<Value, Value> {
    let name = if step["op"] == "stack" { b"mbstring.regex_stack_limit".as_slice() } else { b"mbstring.regex_retry_limit" };
    let value = step["value"].as_i64().unwrap().to_string();
    let args = [MbArgV1::string(name), MbArgV1::string(value.as_bytes())];
    let host = MbIniHostV1 { version: 1, size: std::mem::size_of::<MbIniHostV1>() as u32,
        context: std::ptr::null_mut(), diagnostic: Some(ini_diagnostic) };
    let mut result = MbResultV1::default();
    assert_eq!(unsafe { elephc_mbstring_ini_v1(INI_SET, args.as_ptr(), 2, &host, &mut result) }, 0);
    decode_wire(result)
}

/// Builds real PHP argument cells from one independently captured operation step.
fn arguments(step: &Value) -> (RuntimeBuiltinId, Vec<Php>) {
    let argument = |name| bytes(&step[name]).map_or(Php::Null, Php::String);
    match step["op"].as_str().unwrap() {
        "encoding" => (RuntimeBuiltinId::MbRegexEncoding, vec![argument("value")]),
        "options" => (RuntimeBuiltinId::MbRegexSetOptions, vec![argument("value")]),
        "init" => (RuntimeBuiltinId::MbEregSearchInit, vec![argument("subject"), argument("pattern"), argument("options")]),
        "match" => (RuntimeBuiltinId::MbEregMatch, vec![argument("pattern"), argument("subject"), argument("options")]),
        "split" => (RuntimeBuiltinId::MbSplit, vec![argument("pattern"), argument("subject"), Php::Int(step["limit"].as_i64().unwrap())]),
        "replace" | "ireplace" => (if step["op"] == "replace" { RuntimeBuiltinId::MbEregReplace } else { RuntimeBuiltinId::MbEregiReplace },
            vec![argument("pattern"), argument("replacement"), argument("subject"), argument("options")]),
        "search" => (RuntimeBuiltinId::MbEregSearch, vec![argument("pattern"), argument("options")]),
        "pos" => (RuntimeBuiltinId::MbEregSearchPos, vec![argument("pattern"), argument("options")]),
        "regs" => (RuntimeBuiltinId::MbEregSearchRegs, vec![argument("pattern"), argument("options")]),
        "getregs" => (RuntimeBuiltinId::MbEregSearchGetregs, vec![]),
        "setpos" => (RuntimeBuiltinId::MbEregSearchSetpos, vec![Php::Int(step["value"].as_i64().unwrap())]),
        operation => panic!("unknown replay operation {operation}"),
    }
}

/// Executes one public operation and observes state only after its real callbacks and cleanup finish.
fn run(step: &Value) -> Value {
    if matches!(step["op"].as_str(), Some("ereg" | "eregi")) { return capture::run(step); }
    let mut replay = Replay { host: Host::new(""), step: step.clone(), warnings: Vec::new(), callbacks: Vec::new() };
    let value = if matches!(step["op"].as_str().unwrap(), "stack" | "retry") { limit(step) } else {
        let (operation, args) = arguments(step);
        let pointers: Vec<_> = args.iter().map(|value| (value as *const Php).cast::<c_void>()).collect();
        let mut host = replay.host.table_v3();
        host.base.base.diagnostic = Some(diagnostic);
        let mut result = MbResultV1::default();
        let status = unsafe { elephc_mbstring_invoke_v1(operation.as_u32(), pointers.as_ptr(), pointers.len() as u64,
            0, &host.base.base, &mut result) };
        assert!(replay.host.live.is_empty() && replay.host.errors.is_empty(), "{:?}", replay.host.errors);
        match status {
            0 => decode_wire(result),
            2 => {
                unsafe { elephc_mbstring_release_v1(&mut result); }
                Err(replay.host.pending.clone().expect("callback published a pending error"))
            },
            _ => panic!("unexpected public invocation status {status} for {step}"),
        }
    };
    let mut result = json!({"warnings": replay.warnings, "position": getter(RuntimeBuiltinId::MbEregSearchGetpos),
        "encoding": setting(RuntimeBuiltinId::MbRegexEncoding), "options": setting(RuntimeBuiltinId::MbRegexSetOptions)});
    if step.get("on_warning").is_some() { result["callbacks"] = json!(replay.callbacks); }
    match value { Ok(value) => result["value"] = value, Err(error) => result["error"] = error }
    result
}

/// Compares all 578 PHP traces, including chained errors and nested warning actions, through the public ABI.
#[test]
#[ignore = "requires a managed Oniguruma test prefix or pinned 6.9.10 native development files"]
fn mbstring_invoke_regex_progressive_php_corpus() {
    replay_corpus(include_bytes!("../fixtures/regex_request.jsonl.gz"), 578);
}

/// Replays all split results and warning callbacks through the same public invocation used by AOT and eval.
#[test]
#[ignore = "requires a managed Oniguruma test prefix or pinned 6.9.10 native development files"]
fn mbstring_invoke_regex_split_php_corpus() {
    replay_corpus(include_bytes!("../fixtures/regex_split.jsonl.gz"), 1871);
}

/// Replays replacements, null/false distinctions, and diagnostic callbacks through protected public invocation.
#[test]
#[ignore = "requires a managed Oniguruma test prefix or pinned 6.9.10 native development files"]
fn mbstring_invoke_regex_replace_php_corpus() {
    replay_corpus(include_bytes!("../fixtures/regex_replace.jsonl.gz"), 1431);
}

/// Resets exported request state between independently captured PHP traces and validates every result owner.
fn replay_corpus(fixture: &[u8], expected: usize) {
    assert_eq!(unsafe { elephc_mbstring_regex_provider_v1(&regex_provider::provider()) }, 0);
    let reader = flate2::read::GzDecoder::new(fixture);
    let mut count = 0;
    for line in BufReader::new(reader).lines() {
        elephc_mbstring_reset_v1();
        call(RuntimeBuiltinId::MbRegexSetOptions, &[MbArgV1::string(b"pr")]);
        let case: Value = serde_json::from_str(&line.unwrap()).unwrap();
        for (index, step) in case["steps"].as_array().unwrap().iter().enumerate() {
            assert_eq!(run(step), case["trace"][index], "case {count}, step {index}: {case}");
        }
        count += 1;
    }
    assert_eq!(count, expected);
}
