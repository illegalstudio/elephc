//! Purpose:
//! Verifies shared mbstring call orchestration against PHP ordering and explicit C ABI failures.
//!
//! Called from:
//! - Cargo's focused mbstring integration test harness.
//!
//! Key details:
//! - The real coordinator and operation engine run through an independent callback host.
//! - PHP captures supply strictness, diagnostic/callback order, references, COW, and final state.
//! - Host ownership must balance on success, malformed metadata, callback throws, and cleanup errors.

#[path = "invoke/fixture.rs"]
mod fixture;
#[path = "invoke/entities.rs"]
mod entities;
#[path = "invoke/ini.rs"]
mod ini;
#[path = "invoke/output.rs"]
mod output;
#[path = "invoke/replacement_callback.rs"]
mod replacement_callback;
#[path = "invoke/graph.rs"]
mod graph;
#[path = "invoke/query.rs"]
mod query;
#[path = "invoke/regex.rs"]
mod regex;
#[path = "invoke/regex_request.rs"]
mod regex_request;
#[path = "support/regex_provider.rs"]
#[allow(dead_code)]
mod regex_provider;

use std::ffi::c_void;
use elephc_builtin_contract::{RuntimeBuiltinId, mbstring_abi::{*, invoke::{MbInvokeHostV1, MbInvokeHostV2}}};
use elephc_mbstring::abi::*;
use fixture::*;
use serde_json::{json, Value};

/// Compares all 66 outer argument-order cases with PHP through actual invocation and reentrant callbacks.
#[test]
fn mbstring_invoke_matches_php_outer_order() {
    let cases: Vec<Value> = serde_json::from_str(include_str!("fixtures/coercion_order.json")).unwrap();
    let mut count = 0;
    for case in cases {
        let scenario = case["scenario"].as_str().unwrap();
        if scenario.starts_with("list_") { continue; }
        elephc_mbstring_reset_v1();
        let mut host = Host::new(case["handler"].as_str().unwrap());
        let (operation, values) = arguments(scenario, &mut host);
        let (_, result) = run(operation, &values, case["strict"].as_bool().unwrap(), &mut host);
        assert_eq!(result, case["result"], "{case}");
        assert_eq!(json!(host.trace), case["trace"], "{case}");
        assert_eq!(host.observed(), case["observed"], "{case}");
        assert_eq!(call(RuntimeBuiltinId::MbInternalEncoding, &[]), json!(["string", hex(case["internal"].as_str().unwrap().as_bytes())]), "{case}");
        assert_eq!(call(RuntimeBuiltinId::MbLanguage, &[]), json!(["string", hex(case["language"].as_str().unwrap().as_bytes())]), "{case}");
        assert_eq!(call(RuntimeBuiltinId::MbSubstituteCharacter, &[]), json!(["int", case["substitution"]]), "{case}");
        assert_eq!(host.precision.to_string(), case["precision"].as_str().unwrap(), "{case}");
        assert!(host.live.is_empty(), "unreleased owners: {case}");
        assert!(host.errors.is_empty(), "{:?}: {case}", host.errors);
        assert_eq!(&host.events[..values.len()], vec!["clone"; values.len()]);
        count += 1;
    }
    assert_eq!(count, 66);
}

/// Injects failures at every ownership-bearing stage and verifies complete final cleanup.
#[test]
fn mbstring_invoke_releases_owners_on_callback_failures() {
    for (scenario, callback, occurrence) in [
        ("lossy_before_stringable", "clone", 1), ("lossy_before_stringable", "clone", 3),
        ("stringable_changes_internal", "describe", 1), ("stringable_changes_internal", "stringable", 1),
        ("float_format_before_later_warning", "float", 1), ("lossy_before_stringable", "diagnostic", 1),
        ("array_cow_mutation", "read", 1), ("stringable_changes_internal", "release", 1),
        ("stringable_changes_internal", "release", 2), ("lossy_before_stringable", "release", 4),
    ] {
        for status in [1, 2, 3, 254, -1] {
            elephc_mbstring_reset_v1();
            let mut host = Host::new("observe");
            host.fault = Some(Fault { callback, occurrence, status, malformed: false });
            let (operation, values) = arguments(scenario, &mut host);
            let (actual, _) = run(operation, &values, false, &mut host);
            assert_eq!(actual, if status == 2 && callback != "read" { 2 } else { 1 }, "{scenario} {callback} {status}");
            assert!(host.live.is_empty(), "{scenario} {callback} {status}");
            assert!(host.errors.is_empty(), "{:?}", host.errors);
        }
    }
    for (scenario, callback) in [("stringable_changes_internal", "clone"), ("stringable_changes_internal", "describe"),
        ("stringable_changes_internal", "stringable"), ("float_format_before_later_warning", "float")] {
        elephc_mbstring_reset_v1();
        let mut host = Host::new("observe");
        host.fault = Some(Fault { callback, occurrence: 1, status: 0, malformed: true });
        let (operation, values) = arguments(scenario, &mut host);
        assert_eq!(run(operation, &values, false, &mut host).0, 1, "{callback}");
        assert!(host.live.is_empty());
        assert!(host.errors.is_empty(), "{:?}", host.errors);
    }
}

/// Validates arity before reading poisoned arguments/table pointers and rejects malformed host contracts.
#[test]
fn mbstring_invoke_validates_arity_and_host_metadata() {
    let op = RuntimeBuiltinId::MbStrlen.as_u32();
    let mut output = MbResultV1::default();
    for count in [0, 3, u64::MAX] {
        let status = unsafe { elephc_mbstring_invoke_v1(op, std::ptr::dangling(), count, 0, std::ptr::dangling(), &mut output) };
        assert_eq!(status, 0);
        assert_eq!(result(output)[1], "ArgumentCountError");
        output = MbResultV1::default();
    }
    elephc_mbstring_reset_v1();
    let mut host = Host::new("observe");
    let mut tables = Vec::new();
    let valid = host.table();
    tables.push(MbInvokeHostV1 { version: 2, ..valid });
    tables.push(MbInvokeHostV1 { size: 0, ..valid });
    tables.push(MbInvokeHostV1 { clone_value: None, ..valid });
    tables.push(MbInvokeHostV1 { describe_value: None, ..valid });
    tables.push(MbInvokeHostV1 { stringable: None, ..valid });
    tables.push(MbInvokeHostV1 { format_float: None, ..valid });
    tables.push(MbInvokeHostV1 { diagnostic: None, ..valid });
    tables.push(MbInvokeHostV1 { release_owner: None, ..valid });
    tables.push(MbInvokeHostV1 { array_next: None, ..valid });
    let null: *const c_void = std::ptr::null();
    for table in tables {
        assert_eq!(unsafe { elephc_mbstring_invoke_v1(op, &null, 1, 0, &table, &mut output) }, 1);
        unsafe { elephc_mbstring_release_v1(&mut output); }
    }
    for (op, strict, args, table) in [(op, 2, &null as *const _, &valid as *const _),
        (u32::MAX, 0, &null, &valid), (RuntimeBuiltinId::Abs.as_u32(), 0, &null, &valid),
        (op, 0, std::ptr::null(), &valid), (op, 0, &null, std::ptr::null())] {
        assert_eq!(unsafe { elephc_mbstring_invoke_v1(op, args, 1, strict, table, &mut output) }, 1);
        unsafe { elephc_mbstring_release_v1(&mut output); }
    }
    assert!(host.events.is_empty());
    assert_eq!(unsafe { elephc_mbstring_invoke_v1(op, &null, 1, 0, &valid, std::ptr::null_mut()) }, 1);
    assert_eq!(unsafe { elephc_mbstring_invoke_v1(op, &null, 1, 0, &valid, &mut output) }, 0);
    assert_eq!(result(output), json!(["int", 0]));
    assert!(host.live.is_empty());
    assert!(host.errors.is_empty());
    assert_eq!(run(RuntimeBuiltinId::MbStrlen, &[Php::Bool(true)], false, &mut host).1, json!(["int", 1]));
}

/// Validates callback-driven auto expansion, later reference reads, and failure short-circuiting.
#[test]
fn mbstring_invoke_encoding_lists_are_incremental() {
    use std::{cell::RefCell, rc::Rc};
    elephc_mbstring_reset_v1();
    let mut host = Host::new("");
    let later = Rc::new(RefCell::new(string(b"bad")));
    let values = vec![Php::Array(Rc::new(vec![
        object("first", b"ASCII", Action::Assign(later.clone(), Box::new(string(b"UTF-8")))),
        Php::Reference(later),
        object("language", b"auto", Action::Language(b"Japanese")),
    ]))];
    assert_eq!(run(RuntimeBuiltinId::MbDetectOrder, &values, true, &mut host), (0, json!(["bool", true])));
    let expected = ["ASCII", "UTF-8", "ASCII", "JIS", "UTF-8", "EUC-JP", "SJIS"]
        .map(|name| hex(name.as_bytes()));
    assert_eq!(call(RuntimeBuiltinId::MbDetectOrder, &[]), json!(["array", expected]));
    assert!(host.live.is_empty() && host.errors.is_empty());
    let mut host = Host::new("");
    let values = vec![Php::Array(Rc::new(vec![object("invalid", b"bad", Action::None),
        object("later", b"UTF-8", Action::Throw)]))];
    let (_, result) = run(RuntimeBuiltinId::MbDetectOrder, &values, false, &mut host);
    assert_eq!(result[1], "ValueError");
    assert_eq!(host.trace, vec![json!(["stringify", "invalid"])]);
    assert!(host.live.is_empty() && host.errors.is_empty());
}

/// Consumes copied entry and string owners across every callback's fatal or pending failure.
#[test]
fn mbstring_invoke_encoding_list_owners_survive_failures() {
    use std::rc::Rc;
    let values = vec![Php::Array(Rc::new(vec![object("entry", b"UTF-8", Action::None)]))];
    let mut baseline = Host::new("");
    assert_eq!(run(RuntimeBuiltinId::MbDetectOrder, &values, false, &mut baseline).0, 0);
    let callbacks = baseline.events.clone();
    for (position, &callback) in callbacks.iter().enumerate() {
        let occurrence = callbacks[..=position].iter().filter(|&&name| name == callback).count();
        for status in [1, 2] {
            let mut host = Host::new("");
            host.fault = Some(Fault { callback, occurrence, status, malformed: false });
            assert_eq!(run(RuntimeBuiltinId::MbDetectOrder, &values, false, &mut host).0, status,
                "{callback} occurrence {occurrence}");
            assert!(host.live.is_empty(), "{callback} occurrence {occurrence} retained an owner");
            assert!(host.errors.is_empty(), "{:?}", host.errors);
        }
    }
    let mut host = Host::new("");
    host.fault = Some(Fault { callback: "array_value", occurrence: 1, status: 0, malformed: true });
    assert_eq!(run(RuntimeBuiltinId::MbDetectOrder, &values, false, &mut host).0, 1);
    assert!(host.live.is_empty() && host.errors.is_empty());
}

/// Rejects incomplete V2 tables without callbacks while retaining V1 getter and string-setter support.
#[test]
fn mbstring_invoke_encoding_list_host_versions() {
    let op = RuntimeBuiltinId::MbDetectOrder.as_u32();
    let mut host = Host::new("");
    let valid = host.table_v2();
    let mut output = MbResultV1::default();
    for table in [MbInvokeHostV2 { array_value: None, ..valid },
        MbInvokeHostV2 { base: MbInvokeHostV1 { size: 72, ..valid.base }, ..valid },
        MbInvokeHostV2 { base: MbInvokeHostV1 { version: 3, ..valid.base }, ..valid }] {
        assert_eq!(unsafe { elephc_mbstring_invoke_v1(op, std::ptr::null(), 0, 0, &table.base, &mut output) }, 1);
        unsafe { elephc_mbstring_release_v1(&mut output); }
    }
    assert!(host.events.is_empty());
    let table = host.table();
    let value = string(b"UTF-8");
    let argument = (&value as *const Php).cast::<c_void>();
    assert_eq!(unsafe { elephc_mbstring_invoke_v1(op, &argument, 1, 0, &table, &mut output) }, 0);
    assert_eq!(result(output), json!(["bool", true]));
    let mut output = MbResultV1::default();
    assert_eq!(unsafe { elephc_mbstring_invoke_v1(op, std::ptr::null(), 0, 0, &table, &mut output) }, 0);
    assert_eq!(result(output), json!(["array", [hex(b"UTF-8")]]));
    let value = Php::Array(std::rc::Rc::new(vec![string(b"UTF-8")]));
    let argument = (&value as *const Php).cast::<c_void>();
    let mut output = MbResultV1::default();
    assert_eq!(unsafe { elephc_mbstring_invoke_v1(op, &argument, 1, 0, &table, &mut output) }, 1);
    unsafe { elephc_mbstring_release_v1(&mut output); }
    assert!(host.live.is_empty() && host.errors.is_empty());
}

/// Uses catalog identity rather than equal contents to choose candidate weighting through the real coordinator.
#[test]
fn mbstring_invoke_detect_encoding_catalog_identity() {
    use std::rc::Rc;
    elephc_mbstring_reset_v1();
    let catalog = Rc::new(elephc_mbstring::encoding::Encoding::all()
        .map(|encoding| string(encoding.name().as_bytes())).collect::<Vec<_>>());
    let mut host = Host::new("");
    host.catalog = Some(catalog.clone());
    let input = string(b"caff\xa8\xa8 Stra?e");
    let args = [input.clone(), Php::Array(catalog.clone())];
    assert_eq!(run(RuntimeBuiltinId::MbDetectEncoding, &args, false, &mut host),
        (0, json!(["string", hex(b"GB18030")])));
    let args = [input, Php::Array(Rc::new((*catalog).clone()))];
    assert_eq!(run(RuntimeBuiltinId::MbDetectEncoding, &args, false, &mut host),
        (0, json!(["string", hex(b"SJIS")])));
    assert!(host.live.is_empty() && host.errors.is_empty());
}

/// Keeps V1 list-result framing stable while V2 hosts opt into cached native materialization.
#[test]
fn mbstring_invoke_catalog_result_versions() {
    for extended in [false, true] {
        let mut host = Host::new("");
        let original = host.table();
        let extended_host = host.table_v2();
        let table = if extended { &extended_host.base } else { &original };
        let mut output = MbResultV1::default();
        let status = unsafe { elephc_mbstring_invoke_v1(RuntimeBuiltinId::MbListEncodings.as_u32(),
            std::ptr::null(), 0, 0, table, &mut output) };
        assert_eq!(status, 0);
        assert_eq!(output.kind, if extended { RESULT_ENCODING_CATALOG } else { RESULT_STRING_ARRAY });
        assert_eq!(output.value, 79);
        unsafe { elephc_mbstring_release_v1(&mut output); }
        assert!(host.live.is_empty() && host.errors.is_empty());
    }
}
