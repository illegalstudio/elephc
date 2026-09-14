//! Purpose:
//! Compares entity-map invocation with PHP and exercises protected diagnostic failures.
//!
//! Called from:
//! - The shared invocation integration test binary.
//!
//! Key details:
//! - Oracle cases include numeric prefixes, overflow, nonfinite values, and rejected maps.
//! - Error-handler callbacks may mutate later references or terminate map conversion.

use super::*;
use std::{cell::RefCell, rc::Rc};

/// Decodes exact byte strings captured from PHP without assuming UTF-8 text.
fn unhex(value: &Value) -> Vec<u8> {
    let text = value.as_str().unwrap();
    (0..text.len()).step_by(2).map(|index| u8::from_str_radix(&text[index..index + 2], 16).unwrap()).collect()
}

/// Checks every captured cast and diagnostic sequence in both strict and weak callers.
#[test]
fn mbstring_invoke_entity_map_casts_match_php() {
    let cases: Vec<Value> = serde_json::from_str(include_str!("../fixtures/entity_maps.json")).unwrap();
    assert_eq!(cases.len(), 86);
    for case in cases {
        for strict in [false, true] {
            elephc_mbstring_reset_v1();
            let value = &case["input"][1];
            let entry = match case["input"][0].as_str().unwrap() {
                "null" => Php::Null, "bool" => Php::Bool(value.as_bool().unwrap()),
                "int" => Php::Int(value.as_i64().unwrap()), "string" => string(&unhex(value)),
                "float" => Php::Float(f64::from_bits(u64::from_str_radix(value.as_str().unwrap(), 16).unwrap())),
                "array" => Php::Array(Rc::new(vec![])),
                "object" => object("rejected", b"123", Action::Throw),
                kind => panic!("unhandled oracle kind {kind}"),
            };
            let decode = case["decode"].as_bool().unwrap();
            let operation = if decode { RuntimeBuiltinId::MbDecodeNumericentity } else { RuntimeBuiltinId::MbEncodeNumericentity };
            let input = if decode { b"&#65;&#29483;".as_slice() } else { "A猫".as_bytes() };
            let args = vec![string(input), Php::Array(Rc::new(vec![Php::Int(0), Php::Int(0x10ffff), entry, Php::Int(0xffffffff)])), string(b"UTF-8")];
            let mut host = Host::new("");
            assert_eq!(run(operation, &args, strict, &mut host), (0, case["result"].clone()), "{case}");
            assert_eq!(json!(host.trace), case["trace"], "{case}");
            assert!(host.live.is_empty() && host.errors.is_empty(), "{case}: {:?}", host.errors);
        }
    }
}

/// Preserves encoding selection while map diagnostics update substitution and later referenced values.
#[test]
fn mbstring_invoke_entity_map_diagnostic_reentry() {
    elephc_mbstring_reset_v1();
    let later = Rc::new(RefCell::new(Php::Int(0)));
    let args = vec![string(b"A"), Php::Array(Rc::new(vec![Php::Int(0), Php::Int(100), Php::Float(1.5), Php::Reference(later.clone())]))];
    let mut host = Host::new("entity_map");
    host.observer = Some(("mask", later));
    assert_eq!(run(RuntimeBuiltinId::MbEncodeNumericentity, &args, false, &mut host), (0, json!(["string", hex(b"&#66;")])));
    assert_eq!(host.observed(), json!({"mask": 255}));
    assert!(host.live.is_empty() && host.errors.is_empty());

    elephc_mbstring_reset_v1();
    let args = vec![string(b"A\xff"), Php::Array(Rc::new(vec![Php::Int(0), Php::Int(100), Php::Float(0.5), Php::Int(255)]))];
    let mut host = Host::new("mutate");
    assert_eq!(run(RuntimeBuiltinId::MbEncodeNumericentity, &args, false, &mut host), (0, json!(["string", hex(b"&#65;!")])));
    assert!(host.live.is_empty() && host.errors.is_empty());
}

/// Stops after an exception from a map diagnostic and consumes owners on all injected callback failures.
#[test]
fn mbstring_invoke_entity_map_callback_failures() {
    let args = vec![string(b"A"), Php::Array(Rc::new(vec![Php::Int(0), Php::Int(100), Php::Float(1.5), Php::Int(255)]))];
    elephc_mbstring_reset_v1();
    let mut host = Host::new("throw");
    assert_eq!(run(RuntimeBuiltinId::MbEncodeNumericentity, &args, false, &mut host).0, 2);
    assert_eq!(host.events.iter().filter(|&&event| event == "array_value").count(), 3);
    assert!(host.live.is_empty() && host.errors.is_empty());
    for (callback, occurrence) in [("read", 1), ("array_value", 1), ("array_value", 3), ("describe", 5), ("diagnostic", 1), ("release", 3)] {
        for status in [1, 2, -1] {
            elephc_mbstring_reset_v1();
            let mut host = Host::new("");
            host.fault = Some(Fault { callback, occurrence, status, malformed: false });
            assert_eq!(run(RuntimeBuiltinId::MbEncodeNumericentity, &args, false, &mut host).0,
                if status == 2 && callback != "read" { 2 } else { 1 }, "{callback} {occurrence} {status}");
            assert!(host.live.is_empty() && host.errors.is_empty(), "{callback}: {:?}", host.errors);
        }
    }
}
