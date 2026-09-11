//! Purpose:
//! Replays independent PHP query reentry through the shared V5 invocation boundary.
//!
//! Called from:
//! - The focused mbstring invoke integration binary.
//!
//! Key details:
//! - A separate live table model consumes normalized steps without calling the name parser.
//! - Protected callbacks reenter actual shared request APIs and track every published owner.
//! - Public native/eval adapters require their own execution tests after this host contract.

use super::*;
use std::{cell::RefCell, rc::Rc, io::{BufRead, BufReader}};
use elephc_builtin_contract::mbstring_abi::{invoke::*, host::*, coercion::MbHostStringV1, ini::*};
use flate2::read::GzDecoder;

#[path = "query/storage.rs"]
mod storage;
#[path = "query/callbacks.rs"]
mod callbacks;
use storage::Output;

/// Prefix-compatible callback host with independent output identity and oracle-visible state.
#[repr(C)]
struct Replay {
    host: Host, case: Value, output: Output, copy: Output, events: Vec<Value>, writers: usize,
    original: *const c_void, filter: bool, handling: bool, copy_reference: bool,
}

impl Replay {
    /// Creates one query invocation without borrowing the shared request or retaining an old owner.
    fn new(case: Value) -> Self {
        Self { host: Host::new(""), case, output: Output::String(b"old".to_vec()), copy: Output::Null,
            events: Vec::new(), writers: 0, original: std::ptr::null(), filter: false, handling: false, copy_reference: false }
    }

    /// Builds a complete version-five table while preserving the established value-callback prefix.
    fn table(&mut self) -> MbInvokeHostV5 {
        let mut base = self.host.table_v3();
        base.base.base.version = 5;
        base.base.base.size = std::mem::size_of::<MbInvokeHostV5>() as u32;
        base.base.base.diagnostic = Some(callbacks::diagnostic);
        MbInvokeHostV5 { base: MbInvokeHostV4 { base, capture_initialize: Some(callbacks::initialize),
            capture_fill: Some(callbacks::unexpected_fill), capture_release: Some(callbacks::release) },
            query_configuration: Some(callbacks::configuration), query_filter: self.filter.then_some(callbacks::filter),
            query_register: Some(callbacks::register) }
    }

    /// Calls the real typed coordinator and verifies every argument, metadata, and writer lease ends.
    fn invoke(&mut self, values: &[Php], strict: u32) -> (i32, Value) {
        let pointers = values.iter().map(|value| (value as *const Php).cast::<c_void>()).collect::<Vec<_>>();
        self.original = pointers.get(1).copied().unwrap_or(std::ptr::null());
        let table = self.table();
        let mut output = MbResultV1::default();
        let status = unsafe { elephc_mbstring_invoke_v1(RuntimeBuiltinId::MbParseStr.as_u32(),
            pointers.as_ptr(), pointers.len() as u64, strict, &table.base.base.base.base, &mut output) };
        let value = result(output);
        assert!(self.host.live.is_empty(), "query owners leaked: {:?}", self.host.events);
        assert_eq!(self.writers, 0, "query writer leaked");
        assert!(self.host.errors.is_empty(), "{:?}", self.host.errors);
        (status, value)
    }

    /// Records host activity and selects an optional fault at its exact invocation count.
    fn enter(&mut self, name: &'static str) -> Option<Fault> {
        self.host.events.push(name);
        let _ = call(RuntimeBuiltinId::MbHttpInput, &[]);
        self.host.fault.filter(|fault| fault.callback == name && self.host.events.iter().filter(|&&event| event == name).count() == fault.occurrence)
    }
}

/// Restores arbitrary PHP source bytes from a fixture's hexadecimal form.
fn unhex(value: &str) -> Vec<u8> { (0..value.len()).step_by(2).map(|index| u8::from_str_radix(&value[index..index + 2], 16).unwrap()).collect() }

/// Encodes a string using the reentry fixture's binary-safe representation.
fn encoded(value: &[u8]) -> Value { json!({"bytes": hex(value)}) }

/// Reads one scalar setting through the ordinary public shared operation dispatcher.
fn setting(operation: RuntimeBuiltinId, args: &[MbArgV1]) -> Value {
    let output = call(operation, args);
    if output[0] == "string" { json!(String::from_utf8(unhex(output[1].as_str().unwrap())).unwrap()) }
    else { output[1].clone() }
}

/// Applies real shared INI semantics while the fixture intentionally suppresses deprecations.
fn set_ini(name: &[u8], value: &[u8]) {
    let arguments = [MbArgV1::string(name), MbArgV1::string(value)];
    let host = MbIniHostV1 { version: 1, size: std::mem::size_of::<MbIniHostV1>() as u32,
        context: std::ptr::null_mut(), diagnostic: Some(callbacks::ignore_diagnostic) };
    let mut output = MbResultV1::default();
    assert_eq!(unsafe { elephc_mbstring_ini_v1(INI_SET, arguments.as_ptr(), 2, &host, &mut output) }, 0);
    unsafe { elephc_mbstring_release_v1(&mut output); }
}

/// Restores the exact request state used by one isolated PHP worker.
fn configure(case: &Value) {
    elephc_mbstring_reset_v1();
    set_ini(b"mbstring.http_input", case["encodings"].as_str().unwrap().as_bytes());
    set_ini(b"mbstring.strict_detection", if case["strict"] == true { b"1" } else { b"0" });
    call(RuntimeBuiltinId::MbInternalEncoding, &[MbArgV1::string(case["internal"].as_str().unwrap().as_bytes())]);
}

/// Compares callback order, live output/copies, pending exceptions, and final settings with PHP.
#[test]
fn mbstring_query_reentry_matches_php() {
    let fixture = include_bytes!("../fixtures/parse_str_reentry.jsonl.gz");
    let reader = BufReader::new(GzDecoder::new(fixture.as_slice()));
    let mut count = 0;
    for line in reader.lines() {
        let oracle: Value = serde_json::from_str(&line.unwrap()).unwrap();
        let case = &oracle["case"];
        configure(case);
        let mut replay = Replay::new(case.clone());
        let (status, returned) = replay.invoke(&[string(&unhex(case["query"].as_str().unwrap())), Php::Null], 0);
        if status == 2 { replay.events.push(json!(["exception", "RuntimeException", replay.host.pending.as_ref().unwrap()[0]])); }
        else { assert_eq!(status, 0, "{case}"); replay.events.push(json!(["return", returned[1]])); }
        assert_eq!(json!(replay.events), oracle["events"], "events: {case}");
        assert_eq!(replay.output.snapshot(), oracle["output"], "output: {case}");
        assert_eq!(if replay.copy_reference { replay.output.snapshot() } else { replay.copy.snapshot() }, oracle["copy"], "copy: {case}");
        for (field, operation, args) in [
            ("identified", RuntimeBuiltinId::MbHttpInput, vec![]),
            ("string_source", RuntimeBuiltinId::MbHttpInput, vec![MbArgV1::string(b"S")]),
            ("illegal", RuntimeBuiltinId::MbGetInfo, vec![MbArgV1::string(b"illegal_chars")]),
            ("internal", RuntimeBuiltinId::MbInternalEncoding, vec![]),
            ("configured", RuntimeBuiltinId::MbHttpInput, vec![MbArgV1::string(b"L")]),
            ("substitute", RuntimeBuiltinId::MbSubstituteCharacter, vec![]),
        ] { assert_eq!(setting(operation, &args), oracle[field], "{field}: {case}"); }
        count += 1;
    }
    assert_eq!(count, 94);
}

/// Supplies one ordinary request with successful parsing and no user diagnostic action.
fn ordinary_case() -> Value {
    json!({"encodings": "UTF-8", "strict": false, "internal": "UTF-8", "max_vars": 1000,
        "max_nesting": 64, "seed": "scalar", "action": "none"})
}

/// Replays complete PHP parsing results through configuration and live registration callbacks.
#[test]
fn mbstring_query_v5_matches_php_fields() {
    let fixture = include_bytes!("../fixtures/parse_str.jsonl.gz");
    let reader = BufReader::new(GzDecoder::new(fixture.as_slice()));
    let mut count = 0;
    for line in reader.lines() {
        let oracle: Value = serde_json::from_str(&line.unwrap()).unwrap();
        let mut case = ordinary_case();
        for field in ["strict", "max_vars", "max_nesting", "display_errors", "separator"] { case[field] = oracle[field].clone(); }
        case["encodings"] = json!(oracle["encodings"].as_array().unwrap().iter().map(|name| name.as_str().unwrap()).collect::<Vec<_>>().join(","));
        case["internal"] = oracle["to"].clone();
        configure(&case);
        let substitute = if let Some(value) = oracle["substitute"].as_i64() { MbArgV1::integer(value) }
            else { MbArgV1::string(oracle["substitute"].as_str().unwrap().as_bytes()) };
        call(RuntimeBuiltinId::MbSubstituteCharacter, &[substitute]);
        let mut replay = Replay::new(case);
        let (status, result) = replay.invoke(&[string(&unhex(oracle["query"].as_str().unwrap())), Php::Null], 0);
        assert_eq!(status, 0, "{oracle}");
        assert_eq!(result, json!(["bool", oracle["result"]]), "{oracle}");
        assert_eq!(replay.output.snapshot(), oracle["output"], "{oracle}");
        let warnings = replay.events.iter().filter(|event| event[0] == "warning").map(|event| json!([event[1], event[2]])).collect::<Vec<_>>();
        assert_eq!(json!(warnings), oracle["warnings"], "{oracle}");
        assert_eq!(setting(RuntimeBuiltinId::MbHttpInput, &[]), oracle["identified"], "{oracle}");
        assert_eq!(setting(RuntimeBuiltinId::MbGetInfo, &[MbArgV1::string(b"illegal_chars")]), oracle["illegal"], "{oracle}");
        count += 1;
    }
    assert_eq!(count, 17904);
}

/// Releases configuration/filter byte owners and output writers on every protected failure class.
#[test]
fn mbstring_query_callback_failures_release_owners() {
    for callback in ["query_initialize", "query_config_entry", "query_config_field", "query_filter", "query_register", "query_release"] {
        for status in [1, 2, 17] {
            let case = ordinary_case();
            configure(&case);
            let mut replay = Replay::new(case);
            replay.filter = true;
            replay.host.fault = Some(Fault { callback, occurrence: 1, status, malformed: false });
            let result = replay.invoke(&[string(b"a=one&b=two"), Php::Int(7)], 0);
            assert_eq!(result.0, if status == 2 { 2 } else { 1 }, "{callback}: {status}");
            assert!(replay.host.events.contains(&callback));
        }
        if callback == "query_release" { continue; }
        let case = ordinary_case();
        configure(&case);
        let mut replay = Replay::new(case);
        replay.filter = true;
        replay.host.fault = Some(Fault { callback, occurrence: 1, status: 0, malformed: true });
        assert_eq!(replay.invoke(&[string(b"a=one"), Php::Null], 0).0, 1, "malformed {callback}");
    }
}

/// Retains a pending destructor exception when a later host callback reports an unrecoverable failure.
#[test]
fn mbstring_query_pending_exception_survives_host_failure() {
    let mut case = ordinary_case();
    case["seed"] = json!("throw");
    for callback in ["query_config_entry", "query_config_field", "query_filter", "query_register"] {
        configure(&case);
        let mut replay = Replay::new(case.clone());
        replay.filter = true;
        replay.host.fault = Some(Fault { callback, occurrence: 1, status: 1, malformed: false });
        assert_eq!(replay.invoke(&[string(b"a=value"), Php::Null], 0).0, 2, "{callback}");
        assert_eq!(replay.host.pending, Some(json!(["initialization"])));
    }
}

/// Keeps accepted filtered values and later fields after a filter callback leaves an exception pending.
#[test]
fn mbstring_query_filter_values_survive_pending_exception() {
    for rejected in [false, true] {
        let mut case = ordinary_case();
        case["filter_reject"] = json!(rejected);
        configure(&case);
        let mut replay = Replay::new(case);
        replay.filter = true;
        replay.host.fault = Some(Fault { callback: "query_filter", occurrence: 1, status: 2, malformed: false });
        assert_eq!(replay.invoke(&[string(b"a=one&b=two"), Php::Null], 0).0, 2);
        let expected = if rejected { json!({"array": []}) }
            else { json!({"array": [[encoded(b"a"), encoded(b"ONE")], [encoded(b"b"), encoded(b"TWO")]]}) };
        assert_eq!(replay.output.snapshot(), expected);
        assert_eq!(setting(RuntimeBuiltinId::MbHttpInput, &[]), "UTF-8");
    }
}

/// Rejects wrong arity, source types, and typed output initialization before query-body callbacks.
#[test]
fn mbstring_query_argument_and_output_validation() {
    let case = ordinary_case();
    for values in [vec![], vec![string(b"a=1")], vec![string(b"a=1"), Php::Null, Php::Null]] {
        configure(&case);
        let mut replay = Replay::new(case.clone());
        let (status, result) = replay.invoke(&values, 0);
        assert_eq!(status, 0);
        assert_eq!(result[1], "ArgumentCountError");
        assert!(replay.host.events.is_empty());
    }
    for (source, strict) in [(Php::Array(Rc::new(vec![])), 0), (Php::Int(1), 1)] {
        configure(&case);
        let mut replay = Replay::new(case.clone());
        let (status, result) = replay.invoke(&[source, Php::Null], strict);
        assert_eq!(status, 0);
        assert_eq!(result[1], "TypeError");
        assert!(!replay.host.events.contains(&"query_initialize"));
        assert_eq!(replay.output.snapshot(), encoded(b"old"));
    }
    let mut rejected = case;
    rejected["reject_output"] = json!(true);
    configure(&rejected);
    let mut replay = Replay::new(rejected);
    assert_eq!(replay.invoke(&[string(b"a=1"), Php::Null], 0).0, 2);
    assert!(!replay.host.events.contains(&"query_config_entry"));
    assert_eq!(replay.output.snapshot(), encoded(b"old"));
    assert_eq!(setting(RuntimeBuiltinId::MbHttpInput, &[]), false);
}

/// Rejects old or incomplete callback tables before cloning, pinning, or initializing arguments.
#[test]
fn mbstring_query_requires_complete_v5_host() {
    let values = [string(b"a=1"), Php::Null];
    let args = values.iter().map(|value| (value as *const Php).cast::<c_void>()).collect::<Vec<_>>();
    for (version, size) in [
        (1, std::mem::size_of::<MbInvokeHostV1>()), (2, std::mem::size_of::<MbInvokeHostV2>()),
        (3, std::mem::size_of::<MbInvokeHostV3>()), (4, std::mem::size_of::<MbInvokeHostV4>()),
        (5, std::mem::size_of::<MbInvokeHostV5>()),
    ] {
        let mut replay = Replay::new(ordinary_case());
        let mut table = replay.table();
        table.base.base.base.base.version = version;
        table.base.base.base.base.size = size as u32;
        if version == 5 { table.query_configuration = None; }
        let mut output = MbResultV1::default();
        assert_eq!(unsafe { elephc_mbstring_invoke_v1(RuntimeBuiltinId::MbParseStr.as_u32(), args.as_ptr(), 2,
            0, &table.base.base.base.base, &mut output) }, 1);
        unsafe { elephc_mbstring_release_v1(&mut output); }
        assert!(replay.host.events.is_empty());
        assert!(replay.host.live.is_empty());
    }
}

/// Reads diagnostic policy after host writes can run destructors that change display_errors.
#[test]
fn mbstring_query_diagnostic_policy_follows_mutation() {
    let oracles: Vec<Value> = serde_json::from_str(include_str!("../fixtures/parse_str_display.json")).unwrap();
    for oracle in oracles {
        let display = oracle["new_display"] == 1;
        let mut case = ordinary_case();
        case["encodings"] = json!("ASCII,UTF-8");
        case["strict"] = json!(true);
        case["max_nesting"] = json!(1);
        case["display_errors"] = json!(!display);
        case["display_after_write"] = json!(display);
        configure(&case);
        let mut replay = Replay::new(case);
        assert_eq!(replay.invoke(&[string(b"a[x][y]=%FF"), Php::Null], 0).0, 0);
        let actual = replay.events.iter().filter(|event| event[0] == "warning").map(|event| event[2].clone()).collect::<Vec<_>>();
        let expected = oracle["events"].as_array().unwrap().iter().filter(|event| event[0] == "warning").map(|event| event[1].clone()).collect::<Vec<_>>();
        assert_eq!(actual, expected);
        let write = replay.host.events.iter().position(|event| *event == "query_register").unwrap();
        let policy = replay.host.events.iter().position(|event| *event == "query_config_diagnostic").unwrap();
        assert!(write < policy);
    }
    for status in [1, 2, 17] {
        let mut case = ordinary_case();
        case["max_nesting"] = json!(0);
        configure(&case);
        let mut replay = Replay::new(case);
        replay.host.fault = Some(Fault { callback: "query_config_diagnostic", occurrence: 1, status, malformed: false });
        assert_eq!(replay.invoke(&[string(b"a[x]=value"), Php::Null], 0).0, if status == 2 { 2 } else { 1 });
    }
}

/// Preserves source Stringable effects before output initialization and skips that initialization on throws.
#[test]
fn mbstring_query_source_conversion_precedes_output_initialization() {
    let case = ordinary_case();
    configure(&case);
    let mut replay = Replay::new(case.clone());
    let source = object("QuerySource", b"a=%C3%A9", Action::Internal);
    assert_eq!(replay.invoke(&[source, Php::Null], 0).0, 0);
    assert_eq!(replay.output.snapshot(), json!({"array": [[encoded(b"a"), encoded(b"\xe9")]]}));
    let cast = replay.host.events.iter().position(|event| *event == "stringable").unwrap();
    let initialize = replay.host.events.iter().position(|event| *event == "query_initialize").unwrap();
    assert!(cast < initialize);
    configure(&case);
    let mut replay = Replay::new(case);
    let source = object("QuerySource", b"unused", Action::Throw);
    assert_eq!(replay.invoke(&[source, Php::Null], 0).0, 2);
    assert!(!replay.host.events.contains(&"query_initialize"));
    assert_eq!(replay.output.snapshot(), encoded(b"old"));
}
