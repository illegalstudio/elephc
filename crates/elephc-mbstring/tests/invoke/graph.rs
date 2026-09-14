//! Purpose:
//! Verifies recursive V3 invocation reads, identity preservation, and failure cleanup.
//!
//! Called from:
//! - The focused protected mbstring invocation integration test binary.
//!
//! Key details:
//! - Real conversion runs against an independent host with injected callback failures.
//! - Every copied value and original identity owner must be released before invocation returns.

use std::{cell::RefCell, ffi::c_void, rc::Rc};
use elephc_builtin_contract::{RuntimeBuiltinId, mbstring_abi::{*, array::{ArrayGraph, Key, Value}, invoke::*}};
use elephc_mbstring::abi::*;
use super::fixture::*;

/// Builds a shared nested array whose referenced value changes during source-list preparation.
fn arguments() -> Vec<Php> {
    let reference = Rc::new(RefCell::new(string(b"before")));
    let child = Rc::new(vec![Php::Reference(reference.clone()), Php::Float(-0.0)]);
    vec![Php::Array(Rc::new(vec![Php::Array(child.clone()), Php::Int(7), Php::Array(child)])),
        string(b"UTF-8"), Php::Array(Rc::new(vec![object("change", b"UTF-8",
            Action::Assign(reference, Box::new(string(b"after"))))]))]
}

/// Executes a V3 conversion while retaining the returned wire result for inspection and release.
fn invoke(host: &mut Host, table: Option<MbInvokeHostV3>) -> (i32, MbResultV1) {
    let args = arguments();
    let pointers = args.iter().map(|value| (value as *const Php).cast::<c_void>()).collect::<Vec<_>>();
    let table = table.unwrap_or_else(|| host.table_v3());
    let mut output = MbResultV1::default();
    let status = unsafe { elephc_mbstring_invoke_v1(RuntimeBuiltinId::MbConvertEncoding.as_u32(),
        pointers.as_ptr(), pointers.len() as u64, 0, &table.base.base, &mut output) };
    assert!(host.live.is_empty(), "unreleased owners: {:?}", host.events);
    assert!(host.errors.is_empty(), "{:?}", host.errors);
    (status, output)
}

/// Reads shared input nodes once and converts their independent occurrences with exact scalar bits.
#[test]
fn mbstring_invoke_v3_recursive_graph() {
    elephc_mbstring_reset_v1();
    let mut host = Host::new("");
    let (status, mut output) = invoke(&mut host, None);
    assert_eq!(status, 0);
    assert_eq!(output.kind, RESULT_ARRAY);
    let graph = ArrayGraph::decode(unsafe { std::slice::from_raw_parts(output.bytes, output.len as usize) }).unwrap();
    assert_eq!(graph.arrays(), &[
        vec![(Key::Int(0), Value::Array(1)), (Key::Int(1), Value::Int(7)), (Key::Int(2), Value::Array(2))],
        vec![(Key::Int(0), Value::String(b"after".to_vec())), (Key::Int(1), Value::Float((-0.0_f64).to_bits()))],
        vec![(Key::Int(0), Value::String(b"after".to_vec())), (Key::Int(1), Value::Float((-0.0_f64).to_bits()))],
    ]);
    assert_eq!(host.events.iter().filter(|&&name| name == "pin").count(), 3);
    assert_eq!(host.events.iter().filter(|&&name| name == "graph_value").count(), 7);
    unsafe { elephc_mbstring_release_v1(&mut output); }
}

/// Cleans every published graph value and identity lease on errors, pending throws, and bad cursors.
#[test]
fn mbstring_invoke_v3_graph_failures_release_owners() {
    for (callback, count) in [("pin", 3), ("graph_value", 7), ("describe", 9), ("release", 18)] {
        for occurrence in 1..=count {
            for status in [1, 2, 254] {
                elephc_mbstring_reset_v1();
                let mut host = Host::new("");
                host.fault = Some(Fault { callback, occurrence, status, malformed: false });
                let (actual, mut output) = invoke(&mut host, None);
                let reached = host.events.iter().filter(|&&name| name == callback).count() >= occurrence;
                if reached { assert_eq!(actual, if status == 2 { 2 } else { 1 }, "{callback} {occurrence}"); }
                unsafe { elephc_mbstring_release_v1(&mut output); }
            }
        }
    }
    for occurrence in 1..=7 {
        elephc_mbstring_reset_v1();
        let mut host = Host::new("");
        host.fault = Some(Fault { callback: "graph_value", occurrence, status: 0, malformed: true });
        let (status, mut output) = invoke(&mut host, None);
        assert_eq!(status, 1);
        unsafe { elephc_mbstring_release_v1(&mut output); }
    }
}

/// Rejects incomplete version-three extensions before acquiring any native owners.
#[test]
fn mbstring_invoke_v3_validates_callbacks() {
    elephc_mbstring_reset_v1();
    let mut host = Host::new("");
    let valid = host.table_v3();
    for table in [MbInvokeHostV3 { graph_value: None, ..valid }, MbInvokeHostV3 { pin_value: None, ..valid }] {
        let (status, mut output) = invoke(&mut host, Some(table));
        assert_eq!(status, 1);
        assert!(host.events.is_empty());
        unsafe { elephc_mbstring_release_v1(&mut output); }
    }
}
