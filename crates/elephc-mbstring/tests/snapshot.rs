//! Purpose:
//! Verifies host array snapshots through the real versioned C entry point.
//!
//! Called from:
//! - Cargo's focused mbstring snapshot integration test binary.
//!
//! Key details:
//! - The fake host supplies opaque identities and borrowed values without PHP heap dependencies.
//! - Cycles, aliases, scalar bits, reader failures, and request reentry exercise ABI boundaries.

use std::ffi::c_void;
use elephc_builtin_contract::{RuntimeBuiltinId, mbstring_abi::{*, array::{ArrayGraph, Key, Value}, host::*}};
use elephc_mbstring::abi::{elephc_mbstring_call_v1, elephc_mbstring_release_v1, elephc_mbstring_snapshot_v1};

/// Host storage retained across a snapshot, with optional reader failure/progress violations.
struct Host {
    arrays: Vec<Vec<(MbHostValueV1, MbHostValueV1)>>,
    calls: usize,
    failure: bool,
    stuck: bool,
    reentry: Option<u64>,
}

/// Creates one concrete host value without interpreting its payload bits.
fn host_value(tag: u64, lo: u64, hi: u64) -> MbHostValueV1 { MbHostValueV1 { tag, lo, hi } }

/// Borrows static binary bytes for a host string descriptor.
fn host_string(bytes: &'static [u8]) -> MbHostValueV1 { host_value(HOST_STRING, bytes.as_ptr() as u64, bytes.len() as u64) }

/// Produces ordered entries and probes request-state reentry without panicking across the C ABI.
unsafe extern "C" fn next(
    context: *mut c_void, array: *const MbHostValueV1, cursor: *mut u64,
    key: *mut MbHostValueV1, value: *mut MbHostValueV1,
) -> u64 {
    let host = unsafe { &mut *context.cast::<Host>() };
    host.calls += 1;
    if host.reentry.is_none() {
        let args = [MbArgV1::string("é".as_bytes()), MbArgV1::string(b"UTF-8")];
        let mut result = MbResultV1::default();
        unsafe { elephc_mbstring_call_v1(RuntimeBuiltinId::MbStrlen.as_u32(), args.as_ptr(), 2, &mut result); }
        host.reentry = Some(result.kind);
        unsafe { elephc_mbstring_release_v1(&mut result); }
    }
    if host.failure { return ITER_ERROR; }
    let identity = unsafe { (*array).lo as usize };
    let position = unsafe { *cursor as usize };
    let Some(entries) = host.arrays.get(identity) else { return ITER_ERROR; };
    let Some(&(next_key, next_value)) = entries.get(position) else { return ITER_END; };
    unsafe {
        key.write(next_key);
        value.write(next_value);
        if !host.stuck || position == 0 { *cursor = position as u64 + 1; }
    }
    ITER_ENTRY
}

/// Calls the C snapshot API, copies its wire buffer, and releases all transferred ownership.
fn capture(host: &mut Host) -> (u64, Vec<u8>) {
    let root = host_value(HOST_INDEXED_ARRAY, 0, 0);
    let mut result = MbResultV1::default();
    unsafe { elephc_mbstring_snapshot_v1(&root, Some(next), (host as *mut Host).cast(), &mut result); }
    let bytes = if result.len == 0 { Vec::new() } else { unsafe { std::slice::from_raw_parts(result.bytes, result.len as usize).to_vec() } };
    let kind = result.kind;
    assert_eq!(result.value, 0);
    assert_eq!(result.diagnostics_len, 0);
    unsafe { elephc_mbstring_release_v1(&mut result); elephc_mbstring_release_v1(&mut result); }
    (kind, bytes)
}

/// Builds a host with one indexed node from a sequence of concrete values.
fn host(values: Vec<MbHostValueV1>) -> Host {
    Host { arrays: vec![values.into_iter().enumerate().map(|(index, value)| (host_value(HOST_INT, index as u64, 0), value)).collect()],
        calls: 0, failure: false, stuck: false, reentry: None }
}

/// Verifies all supported value shapes, shared/circular identities, exact keys, and request reentry.
#[test]
fn mbstring_snapshot_preserves_graph_identity_and_scalar_bits() {
    let values = vec![MbHostValueV1::null(), host_value(HOST_BOOL, 1, 0), host_value(HOST_INT, i64::MIN as u64, 0),
        host_value(HOST_FLOAT, 0x8000000000000000, 0), host_value(HOST_FLOAT, 0x7ff8000000000042, 0),
        host_string(b"a\0\xff"), host_value(HOST_ASSOC_ARRAY, 1, 0), host_value(HOST_ASSOC_ARRAY, 1, 0),
        host_value(HOST_INDEXED_ARRAY, 0, 0), host_value(HOST_UNSUPPORTED, 123, 456)];
    let mut host = host(values);
    host.arrays.push(vec![(host_string(b"42"), host_string("日".as_bytes())), (host_value(HOST_INT, 42, 0), MbHostValueV1::null())]);
    let (kind, bytes) = capture(&mut host);
    assert_eq!(kind, RESULT_ARRAY);
    let graph = ArrayGraph::decode(&bytes).unwrap();
    let values = vec![Value::Null, Value::Bool(true), Value::Int(i64::MIN), Value::Float(0x8000000000000000),
        Value::Float(0x7ff8000000000042), Value::String(b"a\0\xff".to_vec()), Value::Array(1), Value::Array(1), Value::Array(0), Value::Unsupported];
    let expected = ArrayGraph::new(0, vec![values.into_iter().enumerate().map(|(index, value)| (Key::Int(index as i64), value)).collect(),
        vec![(Key::String(b"42".to_vec()), Value::String("日".as_bytes().to_vec())), (Key::Int(42), Value::Null)]]).unwrap();
    assert_eq!(graph, expected);
    assert_eq!(host.calls, 14);
    assert_eq!(host.reentry, Some(RESULT_INT));
}

/// Verifies invalid metadata and reader failures never publish a partial successful array.
#[test]
fn mbstring_snapshot_rejects_invalid_reader_results() {
    for invalid in [host_value(HOST_BOOL, 2, 0), host_value(7, 0, 0), host_value(HOST_STRING, 0, 1), host_value(HOST_STRING, 1, u64::MAX)] {
        assert_eq!(capture(&mut host(vec![invalid])), (RESULT_FATAL, vec![]));
    }
    let mut duplicate = host(vec![MbHostValueV1::null(), MbHostValueV1::null()]);
    duplicate.arrays[0][1].0.lo = 0;
    assert_eq!(capture(&mut duplicate), (RESULT_FATAL, vec![]));
    let mut key = host(vec![MbHostValueV1::null()]);
    key.arrays[0][0].0 = MbHostValueV1::null();
    assert_eq!(capture(&mut key), (RESULT_FATAL, vec![]));
    let mut stuck = host(vec![MbHostValueV1::null(), MbHostValueV1::null()]);
    stuck.stuck = true;
    assert_eq!(capture(&mut stuck), (RESULT_FATAL, vec![]));
    assert_eq!(stuck.calls, 2);
    let mut failure = host(vec![MbHostValueV1::null()]);
    failure.failure = true;
    assert_eq!(capture(&mut failure), (RESULT_FATAL, vec![]));
}

/// Verifies a missing reader, null root, or non-array root fails before invoking host code.
#[test]
fn mbstring_snapshot_rejects_invalid_entry_metadata() {
    let mut output = MbResultV1::default();
    let root = host_value(HOST_INT, 0, 0);
    unsafe {
        elephc_mbstring_snapshot_v1(&root, None, std::ptr::null_mut(), &mut output);
        assert_eq!(output.kind, RESULT_FATAL);
        elephc_mbstring_release_v1(&mut output);
        elephc_mbstring_snapshot_v1(std::ptr::null(), Some(next), std::ptr::null_mut(), &mut output);
        assert_eq!(output.kind, RESULT_FATAL);
        elephc_mbstring_release_v1(&mut output);
        elephc_mbstring_snapshot_v1(&root, Some(next), std::ptr::null_mut(), &mut output);
        assert_eq!(output.kind, RESULT_FATAL);
        elephc_mbstring_release_v1(&mut output);
    }
}

/// Copies reader scratch strings before another callback overwrites their borrowed byte ranges.
#[test]
fn mbstring_snapshot_copies_ephemeral_reader_strings() {
    /// Publishes one binary entry, then overwrites the shared scratch buffer at end of iteration.
    unsafe extern "C" fn scratch_next(
        context: *mut c_void, _: *const MbHostValueV1, cursor: *mut u64,
        key: *mut MbHostValueV1, value: *mut MbHostValueV1,
    ) -> u64 {
        let scratch = unsafe { &mut *context.cast::<[u8; 5]>() };
        if unsafe { *cursor } != 0 {
            scratch.fill(b'X');
            return ITER_END;
        }
        *scratch = [b'a', 0, 255, 195, 169];
        unsafe {
            key.write(host_value(HOST_STRING, scratch.as_ptr() as u64, 3));
            value.write(host_value(HOST_STRING, scratch.as_ptr().add(3) as u64, 2));
            *cursor = 1;
        }
        ITER_ENTRY
    }
    let mut scratch = [0_u8; 5];
    let root = host_value(HOST_INDEXED_ARRAY, 0, 0);
    let mut output = MbResultV1::default();
    unsafe { elephc_mbstring_snapshot_v1(&root, Some(scratch_next), (&mut scratch as *mut [u8; 5]).cast(), &mut output); }
    assert_eq!(output.kind, RESULT_ARRAY);
    let graph = ArrayGraph::decode(unsafe { std::slice::from_raw_parts(output.bytes, output.len as usize) });
    unsafe { elephc_mbstring_release_v1(&mut output); }
    assert_eq!(scratch, [b'X'; 5]);
    assert_eq!(graph, ArrayGraph::new(0, vec![vec![(Key::String(b"a\0\xff".to_vec()), Value::String("é".as_bytes().to_vec()))]]));
}
