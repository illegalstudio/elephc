//! Purpose:
//! Verifies encoding checks through the exported mbstring C ABI.
//!
//! Called from:
//! - Cargo's focused mbstring integration test harness.
//!
//! Key details:
//! - Recursive arrays cross the real packed wire boundary, including cycles and binary keys.
//! - Diagnostic ordering and request-error state are PHP-visible behavior.

use elephc_builtin_contract::{RuntimeBuiltinId, mbstring_abi::*};
use elephc_builtin_contract::mbstring_abi::array::{ArrayGraph, Key, Value};
use elephc_mbstring::abi::{elephc_mbstring_call_v1, elephc_mbstring_release_v1, elephc_mbstring_reset_v1};

/// Calls the bridge and copies diagnostics before releasing every owned result buffer.
fn call(operation: RuntimeBuiltinId, args: &[MbArgV1]) -> (u64, i64, Vec<u8>, Vec<u8>) {
    /// Copies a live C result range without reading null empty buffers.
    unsafe fn copy(bytes: *const u8, length: u64) -> Vec<u8> {
        if length == 0 { Vec::new() } else { unsafe { std::slice::from_raw_parts(bytes, length as usize).to_vec() } }
    }
    let mut result = MbResultV1::default();
    unsafe { elephc_mbstring_call_v1(operation.as_u32(), args.as_ptr(), args.len() as u64, &mut result); }
    let value = (result.kind, result.value, unsafe { copy(result.bytes, result.len) },
        unsafe { copy(result.diagnostics, result.diagnostics_len) });
    unsafe { elephc_mbstring_release_v1(&mut result); }
    value
}

/// Checks string validity without incrementing the request counter and preserves null diagnostics.
#[test]
fn mbstring_check_abi_strings_and_request_state() {
    elephc_mbstring_reset_v1();
    let op = RuntimeBuiltinId::MbCheckEncoding;
    let deprecated = b"Deprecated: mb_check_encoding(): Calling mb_check_encoding() without argument is deprecated\n";
    assert_eq!(call(op, &[]), (RESULT_BOOL, 1, vec![], deprecated.to_vec()));
    for (input, valid) in [(b"".as_slice(), 1), ("猫\0é".as_bytes(), 1), (&[255], 0)] {
        assert_eq!(call(op, &[MbArgV1::string(input)]), (RESULT_BOOL, valid, vec![], vec![]));
    }
    assert_eq!(call(op, &[MbArgV1::null()]).1, 1);
    call(RuntimeBuiltinId::MbScrub, &[MbArgV1::string(&[255])]);
    assert_eq!(call(op, &[MbArgV1::null()]), (RESULT_BOOL, 0, vec![], deprecated.to_vec()));
    let bad = call(op, &[MbArgV1::null(), MbArgV1::string(b"bad")]);
    assert_eq!(bad.0, RESULT_VALUE_ERROR);
    assert!(bad.3.is_empty());
    assert_eq!(bad.2, b"mb_check_encoding(): Argument #2 ($encoding) must be a valid encoding, \"bad\" given");
    let base64 = call(op, &[MbArgV1::null(), MbArgV1::string(b"BASE64")]);
    let mut expected = b"Deprecated: mb_check_encoding(): Handling Base64 via mbstring is deprecated; use base64_encode/base64_decode instead\n".to_vec();
    expected.extend_from_slice(deprecated);
    assert_eq!(base64.3, expected);
    assert_eq!(call(op, &[MbArgV1::null(), MbArgV1::string(b"BASE64")]).3, deprecated);
    elephc_mbstring_reset_v1();
    assert_eq!(call(op, &[]).1, 1);
}

/// Traverses real wire cycles and retains the invalid-key versus invalid-value warning order.
#[test]
fn mbstring_check_abi_recursive_arrays() {
    let op = RuntimeBuiltinId::MbCheckEncoding;
    let warning = b"Warning: mb_check_encoding(): Cannot not handle circular references\n";
    for (key, value, warnings) in [
        (Key::Int(0), Value::String(vec![255]), warning.to_vec()),
        (Key::String(vec![255]), Value::Null, vec![]),
        (Key::String(b"ok".to_vec()), Value::Unsupported, warning.to_vec()),
    ] {
        let graph = ArrayGraph::new(0, vec![vec![(key, value), (Key::Int(1), Value::Array(0))]]).unwrap();
        let bytes = graph.encode();
        assert_eq!(call(op, &[MbArgV1::array(&bytes)]), (RESULT_BOOL, 0, vec![], warnings));
        assert_eq!(ArrayGraph::decode(&bytes), Some(graph));
    }
    let graph = ArrayGraph::new(0, vec![
        vec![(Key::Int(0), Value::Array(1)), (Key::Int(1), Value::Array(1))],
        vec![(Key::String(b"a\0b".to_vec()), Value::Float(0x7ff8000000000042)),
            (Key::Int(42), Value::Bool(false)), (Key::String(b"42".to_vec()), Value::Int(i64::MIN))],
    ]).unwrap().encode();
    assert_eq!(call(op, &[MbArgV1::array(&graph)]), (RESULT_BOOL, 1, vec![], vec![]));
}

/// Rejects malformed array wire metadata without treating it as a valid empty PHP array.
#[test]
fn mbstring_check_abi_invalid_array_arguments() {
    let op = RuntimeBuiltinId::MbCheckEncoding;
    for bytes in [vec![], vec![0; 15], vec![0; 16], vec![255; 24]] {
        assert_eq!(call(op, &[MbArgV1::array(&bytes)]).0, RESULT_FATAL);
    }
    let invalid = MbArgV1 { kind: ARG_ARRAY, value: 0, bytes: std::ptr::null(), len: 1 };
    assert_eq!(call(op, &[invalid]).0, RESULT_FATAL);
    assert_eq!(call(op, &[MbArgV1::integer(1)]).0, RESULT_FATAL);
    let empty = ArrayGraph::new(0, vec![vec![]]).unwrap().encode();
    assert_eq!(call(op, &[MbArgV1::array(&empty)]), (RESULT_BOOL, 1, vec![], vec![]));
}
