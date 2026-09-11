//! Purpose:
//! Tests mbstring preparation ABI validation, binary diagnostics, and PHP arity errors.
//!
//! Called from:
//! - Cargo's focused mbstring integration test harness.
//!
//! Key details:
//! - Arity messages come from independent PHP captures for every published shared operation.
//! - Invalid metadata never reaches host pointers or fabricates a successful PHP value.
//! - Inputs remain borrowed, while owned outputs support exactly one release and harmless repeats.

use elephc_builtin_contract::{RuntimeBuiltinId, mbstring_abi::{*, coercion::*, host::*}};
use elephc_mbstring::abi::*;
use serde_json::Value;

/// Builds a scalar descriptor with no borrowed ranges or capability flags.
fn scalar(kind: u64, value: u64) -> MbCoercionInputV1 {
    MbCoercionInputV1 { kind, value, bytes: std::ptr::null(), len: 0, flags: 0 }
}

/// Calls parameter preparation and leaves ownership with the test for explicit inspection/release.
fn prepare(op: u32, index: u32, input: &MbCoercionInputV1, strict: u32) -> MbResultV1 {
    let mut output = MbResultV1::default();
    unsafe { elephc_mbstring_prepare_v1(op, index, input, strict, &mut output); }
    output
}

/// Copies an owned result payload while the bridge allocation is still live.
unsafe fn bytes(output: &MbResultV1) -> &[u8] {
    if output.len == 0 { &[] } else { unsafe { std::slice::from_raw_parts(output.bytes, output.len as usize) } }
}

/// Compares all published arity failures with PHP and accepts each operation's legal counts.
#[test]
fn mbstring_coercion_arity_matches_php() {
    let cases: Vec<Value> = serde_json::from_str(include_str!("fixtures/coercion_arity.json")).unwrap();
    assert_eq!(cases.len(), 75);
    for case in cases {
        let operation = RuntimeBuiltinId::MBSTRING.into_iter().find(|id|
            elephc_builtin_contract::lookup_id(id.builtin_id()).unwrap().name == case["function"].as_str().unwrap()).unwrap();
        let mut output = MbResultV1::default();
        unsafe { elephc_mbstring_arity_v1(operation.as_u32(), case["count"].as_u64().unwrap(), &mut output); }
        assert_eq!(output.kind, PREPARED_ARGUMENT_COUNT_ERROR, "{case}");
        assert_eq!(unsafe { bytes(&output) }, case["message"].as_str().unwrap().as_bytes(), "{case}");
        assert_eq!(case["trace"], serde_json::json!([]));
        unsafe { elephc_mbstring_release_v1(&mut output); }
        unsafe { elephc_mbstring_release_v1(&mut output); }
    }
    for operation in RuntimeBuiltinId::MBSTRING {
        for count in 0..=8 {
            if !operation.supports_arity(count) { continue; }
            let mut output = MbResultV1::default();
            unsafe { elephc_mbstring_arity_v1(operation.as_u32(), count as u64, &mut output); }
            assert_eq!(output.kind, RESULT_BOOL);
            assert_eq!(output.value, 1);
            unsafe { elephc_mbstring_release_v1(&mut output); }
        }
    }
}

/// Rejects unknown operations, malformed flags/ranges, noncanonical scalars, and invalid strictness.
#[test]
fn mbstring_coercion_invalid_metadata() {
    let op = RuntimeBuiltinId::MbStrlen.as_u32();
    let mut cases = vec![scalar(7, 0), scalar(u64::MAX, 0), scalar(HOST_BOOL, 2), scalar(HOST_NULL, 1)];
    for kind in [HOST_INT, HOST_BOOL, HOST_FLOAT, HOST_NULL, HOST_STRING] {
        cases.push(MbCoercionInputV1 { flags: 1, ..scalar(kind, 0) });
    }
    for kind in [HOST_INDEXED_ARRAY, HOST_ASSOC_ARRAY, INPUT_OBJECT, INPUT_RESOURCE] { cases.push(MbCoercionInputV1 { flags: 2, ..scalar(kind, 0) }); }
    for kind in [HOST_STRING, INPUT_OBJECT] {
        cases.push(MbCoercionInputV1 { len: 1, ..scalar(kind, 0) });
        cases.push(MbCoercionInputV1 { bytes: std::ptr::dangling(), len: u64::MAX, ..scalar(kind, 0) });
    }
    cases.push(MbCoercionInputV1 { len: 1, ..scalar(HOST_INT, 0) });
    cases.push(MbCoercionInputV1 { bytes: std::ptr::dangling(), ..scalar(HOST_BOOL, 0) });
    cases.push(scalar(HOST_STRING, 1));
    for input in cases {
        let mut output = prepare(op, 0, &input, 0);
        assert_eq!(output.kind, RESULT_FATAL, "{input:?}");
        unsafe { elephc_mbstring_release_v1(&mut output); }
    }
    for (op, index, strict) in [(u32::MAX, 0, 0), (RuntimeBuiltinId::Abs.as_u32(), 0, 0), (op, u32::MAX, 0), (op, 0, 2)] {
        let mut output = prepare(op, index, &scalar(HOST_INT, 1), strict);
        assert_eq!(output.kind, RESULT_FATAL);
        unsafe { elephc_mbstring_release_v1(&mut output); }
    }
    let mut output = MbResultV1::default();
    unsafe { elephc_mbstring_prepare_v1(op, 0, std::ptr::null(), 0, &mut output); }
    assert_eq!(output.kind, RESULT_FATAL);
    unsafe { elephc_mbstring_release_v1(&mut output); }
    for op in [u32::MAX, RuntimeBuiltinId::Abs.as_u32()] {
        unsafe { elephc_mbstring_arity_v1(op, 0, &mut output); }
        assert_eq!(output.kind, RESULT_FATAL);
        unsafe { elephc_mbstring_release_v1(&mut output); }
    }
    unsafe { elephc_mbstring_prepare_v1(op, 0, std::ptr::null(), 0, std::ptr::null_mut()); }
    unsafe { elephc_mbstring_arity_v1(op, 0, std::ptr::null_mut()); }
}

/// Accepts catalog identity only on array descriptors and preserves ordinary type validation.
#[test]
fn mbstring_coercion_catalog_identity() {
    for kind in [HOST_INDEXED_ARRAY, HOST_ASSOC_ARRAY] {
        let input = MbCoercionInputV1 { flags: INPUT_ENCODING_CATALOG, ..scalar(kind, 0) };
        let mut output = prepare(RuntimeBuiltinId::MbDetectEncoding.as_u32(), 1, &input, 0);
        assert_eq!(output.kind, PREPARED_ARRAY);
        unsafe { elephc_mbstring_release_v1(&mut output); }
        let mut output = prepare(RuntimeBuiltinId::MbStrlen.as_u32(), 0, &input, 0);
        assert_eq!(output.kind, RESULT_TYPE_ERROR);
        unsafe { elephc_mbstring_release_v1(&mut output); }
    }
}

/// Preserves binary class names and embedded newlines in independently framed diagnostic records.
#[test]
fn mbstring_coercion_binary_messages() {
    let class = b"anonymous\0\xff\nclass";
    let input = MbCoercionInputV1 { bytes: class.as_ptr(), len: class.len() as u64, ..scalar(INPUT_OBJECT, u64::MAX) };
    let mut output = prepare(RuntimeBuiltinId::MbStrlen.as_u32(), 0, &input, 0);
    assert_eq!(output.kind, RESULT_TYPE_ERROR);
    assert_eq!(unsafe { bytes(&output) }, [b"mb_strlen(): Argument #1 ($string) must be of type string, ".as_slice(), class, b" given"].concat());
    unsafe { elephc_mbstring_release_v1(&mut output); }
    let number = b"\n33.5\r\n";
    let input = MbCoercionInputV1 { bytes: number.as_ptr(), len: number.len() as u64, ..scalar(HOST_STRING, 0) };
    output = prepare(RuntimeBuiltinId::MbSubstr.as_u32(), 1, &input, 0);
    assert_eq!(output.kind, RESULT_INT);
    assert_eq!(output.value, 33);
    let diagnostics = unsafe { std::slice::from_raw_parts(output.diagnostics, output.diagnostics_len as usize) };
    assert_eq!(decode_diagnostics(diagnostics), Some(vec![(8192,
        b"Implicit conversion from float-string \"\n33.5\r\n\" to int loses precision".as_slice())]));
    unsafe { elephc_mbstring_release_v1(&mut output); }
    unsafe { elephc_mbstring_release_v1(&mut output); }
}
