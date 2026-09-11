//! Purpose:
//! Verifies request accounting and numeric-entity maps through public mbstring C entry points.
//!
//! Called from:
//! - The bridge crate's focused unit test harness.
//!
//! Key details:
//! - Ordinary text repairs leave the PHP counter unchanged; scrub accumulates errors.
//! - Reset and separate threads must not retain another request's rejected units.

use super::*;

/// Runs a byte-producing operation and releases its bridge result before inspecting state.
fn call(operation: RuntimeBuiltinId, input: &[u8]) {
    let args = [MbArgV1::string(input), MbArgV1::string(b"UTF-8")];
    let mut output = MbResultV1::default();
    unsafe { elephc_mbstring_call_v1(operation.as_u32(), args.as_ptr(), args.len() as u64, &mut output); }
    assert_eq!(output.kind, RESULT_STRING);
    unsafe { elephc_mbstring_release_v1(&mut output); }
}

/// Verifies the real C ABI records scrub errors, isolates threads, and resets request counters.
#[test]
fn mbstring_abi_scrub_request_accounting() {
    elephc_mbstring_reset_v1();
    call(RuntimeBuiltinId::MbStrtolower, b"A\xff");
    assert_eq!(REQUEST.with(|state| state.borrow().illegal_chars()), 0);
    call(RuntimeBuiltinId::MbScrub, b"A\xff");
    call(RuntimeBuiltinId::MbScrub, b"\xff\xff");
    assert_eq!(REQUEST.with(|state| state.borrow().illegal_chars()), 3);
    assert_eq!(std::thread::spawn(|| {
        assert_eq!(REQUEST.with(|state| state.borrow().illegal_chars()), 0);
        call(RuntimeBuiltinId::MbScrub, b"\xff");
        REQUEST.with(|state| state.borrow().illegal_chars())
    }).join().unwrap(), 1);
    assert_eq!(REQUEST.with(|state| state.borrow().illegal_chars()), 3);
    elephc_mbstring_reset_v1();
    assert_eq!(REQUEST.with(|state| state.borrow().illegal_chars()), 0);
}

/// Preserves entity-map warnings, ordered keys, empty-input validation, and result ownership in the wire ABI.
#[test]
fn mbstring_abi_numericentity_maps() {
    use elephc_builtin_contract::mbstring_abi::array::{ArrayGraph, Key, Value};
    let entries = vec![(Key::String(b"low".to_vec()), Value::Int(0)),
        (Key::Int(-5), Value::Int(100)), (Key::String(b"offset".to_vec()), Value::String(b"1.5bad".to_vec())),
        (Key::Int(80), Value::Int(255))];
    let valid = ArrayGraph::new(0, vec![entries]).unwrap().encode();
    let invalid = ArrayGraph::new(0, vec![vec![(Key::Int(0), Value::Unsupported)]]).unwrap().encode();
    for (operation, input, expected) in [
        (RuntimeBuiltinId::MbEncodeNumericentity, b"A".as_slice(), b"&#66;".as_slice()),
        (RuntimeBuiltinId::MbDecodeNumericentity, b"&#66;".as_slice(), b"A".as_slice()),
    ] {
        elephc_mbstring_reset_v1();
        let args = [MbArgV1::string(input), MbArgV1::array(&valid)];
        let mut output = MbResultV1::default();
        unsafe { elephc_mbstring_call_v1(operation.as_u32(), args.as_ptr(), args.len() as u64, &mut output); }
        assert_eq!(output.kind, RESULT_STRING);
        assert_eq!(unsafe { std::slice::from_raw_parts(output.bytes, output.len as usize) }, expected);
        assert_eq!(unsafe { std::slice::from_raw_parts(output.diagnostics, output.diagnostics_len as usize) },
            b"Warning: A non-numeric value encountered\nDeprecated: Implicit conversion from float-string \"1.5bad\" to int loses precision\n");
        unsafe { elephc_mbstring_release_v1(&mut output); }
        let args = [MbArgV1::string(b""), MbArgV1::array(&invalid)];
        unsafe { elephc_mbstring_call_v1(operation.as_u32(), args.as_ptr(), args.len() as u64, &mut output); }
        assert_eq!(output.kind, RESULT_VALUE_ERROR);
        let bytes = unsafe { std::slice::from_raw_parts(output.bytes, output.len as usize) };
        assert!(bytes.ends_with(b"must have a multiple of 4 elements"));
        unsafe { elephc_mbstring_release_v1(&mut output); elephc_mbstring_release_v1(&mut output); }
        assert!(output.bytes.is_null() && output.diagnostics.is_null());
    }
}

/// Distinguishes omitted detection strictness from explicit false and validates array identity flags.
#[test]
fn mbstring_abi_detect_encoding_defaults() {
    elephc_mbstring_reset_v1();
    REQUEST.with(|state| state.borrow_mut().set_strict_detection(true));
    let args = [MbArgV1::string(b"\xff"), MbArgV1::null(), MbArgV1::boolean(false)];
    for (count, kind) in [(2, RESULT_BOOL), (3, RESULT_STRING)] {
        let mut output = MbResultV1::default();
        unsafe { elephc_mbstring_call_v1(RuntimeBuiltinId::MbDetectEncoding.as_u32(), args.as_ptr(), count, &mut output); }
        assert_eq!(output.kind, kind);
        if kind == RESULT_BOOL { assert_eq!(output.value, 0); }
        unsafe { elephc_mbstring_release_v1(&mut output); }
    }
    elephc_mbstring_reset_v1();
}
