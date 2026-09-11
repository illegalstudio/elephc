//! Purpose:
//! Verifies recursive conversion results and request accounting through the actual wire ABI.
//!
//! Called from:
//! - Focused mbstring bridge integration tests.
//!
//! Key details:
//! - Graph decoding checks exact key identities and scalar bits before native materialization.
//! - Result buffers remain independently owned and support harmless repeated release.

use elephc_builtin_contract::{RuntimeBuiltinId, mbstring_abi::{*, array::{ArrayGraph, Key, Value}}};
use elephc_mbstring::abi::*;

/// Invokes one shared operation over borrowed arguments and transfers its complete owned result.
fn call(operation: RuntimeBuiltinId, args: &[MbArgV1]) -> MbResultV1 {
    let mut output = MbResultV1::default();
    unsafe { elephc_mbstring_call_v1(operation.as_u32(), args.as_ptr(), args.len() as u64, &mut output); }
    output
}

/// Copies a returned graph while the bridge buffer is still owned by the test.
fn graph(output: &MbResultV1) -> ArrayGraph {
    assert_eq!(output.kind, RESULT_ARRAY);
    ArrayGraph::decode(unsafe { std::slice::from_raw_parts(output.bytes, output.len as usize) }).unwrap()
}

/// Converts keys and child strings while preserving scalar bits and recording rejected characters.
#[test]
fn mbstring_conversion_wire_arrays_and_accounting() {
    elephc_mbstring_reset_v1();
    let input = ArrayGraph::new(0, vec![vec![(Key::String("é".as_bytes().to_vec()), Value::Array(1)), (Key::Int(1), Value::Null)],
        vec![(Key::Int(0), Value::String("猫".as_bytes().to_vec())), (Key::Int(1), Value::Float(0x8000_0000_0000_0000))]]).unwrap().encode();
    let mut output = call(RuntimeBuiltinId::MbConvertEncoding, &[MbArgV1::array(&input), MbArgV1::string(b"ASCII"), MbArgV1::string(b"UTF-8")]);
    assert_eq!(graph(&output), ArrayGraph::new(0, vec![vec![(Key::String(b"?".to_vec()), Value::Array(1)), (Key::Int(1), Value::Null)],
        vec![(Key::Int(0), Value::String(b"?".to_vec())), (Key::Int(1), Value::Float(0x8000_0000_0000_0000))]]).unwrap());
    assert_eq!(output.diagnostics_len, 0);
    unsafe { elephc_mbstring_release_v1(&mut output); elephc_mbstring_release_v1(&mut output); }
    let mut checked = call(RuntimeBuiltinId::MbCheckEncoding, &[]);
    assert_eq!((checked.kind, checked.value), (RESULT_BOOL, 0));
    unsafe { elephc_mbstring_release_v1(&mut checked); }
    elephc_mbstring_reset_v1();
}

/// Keeps converted numeric string keys distinct and preserves first-wins collisions.
#[test]
fn mbstring_conversion_wire_key_identity() {
    let input = ArrayGraph::new(0, vec![vec![(Key::String(b"1\0".to_vec()), Value::String(b"a\0".to_vec())),
        (Key::Int(1), Value::String(b"b\0".to_vec()))]]).unwrap().encode();
    let mut output = call(RuntimeBuiltinId::MbConvertEncoding, &[MbArgV1::array(&input), MbArgV1::string(b"UTF-8"), MbArgV1::string(b"UTF-16LE")]);
    assert_eq!(graph(&output), ArrayGraph::new(0, vec![vec![(Key::String(b"1".to_vec()), Value::String(b"a".to_vec())),
        (Key::Int(1), Value::String(b"b".to_vec()))]]).unwrap());
    unsafe { elephc_mbstring_release_v1(&mut output); }
}
