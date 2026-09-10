//! Purpose:
//! Tests invocation cleanup when Rust panics after native argument owners have been acquired.
//!
//! Called from:
//! - The focused mbstring library test harness.
//!
//! Key details:
//! - A deliberately held request borrow triggers a contained panic at actual operation dispatch.
//! - Minimal non-unwinding callbacks expose allocation counts without touching request state.

use super::*;

/// Copies a concrete fixture descriptor into an owned box and records the acquired owner.
unsafe extern "C" fn clone_value(context: *mut c_void, value: *const c_void, out: *mut *mut c_void) -> i32 {
    let input = unsafe { *value.cast::<MbCoercionInputV1>() };
    unsafe { *out = Box::into_raw(Box::new(input)).cast(); *context.cast::<usize>() += 1; }
    0
}

/// Publishes the copied descriptor without acquiring metadata ownership.
unsafe extern "C" fn describe(
    _context: *mut c_void, value: *const c_void, out: *mut MbCoercionInputV1, _owner: *mut *mut c_void,
) -> i32 {
    unsafe { *out = *value.cast::<MbCoercionInputV1>(); }
    0
}

/// Rejects unexpected object conversion in this exact-string panic regression.
unsafe extern "C" fn stringable(_context: *mut c_void, _value: *const c_void, _out: *mut MbHostStringV1) -> i32 { 1 }

/// Rejects unexpected float conversion in this exact-string panic regression.
unsafe extern "C" fn float(_context: *mut c_void, _bits: u64, _out: *mut MbHostStringV1) -> i32 { 1 }

/// Rejects unexpected diagnostic delivery for the already valid fixture string.
unsafe extern "C" fn diagnostic(_context: *mut c_void, _level: u32, _bytes: *const u8, _len: u64) -> i32 { 1 }

/// Consumes the descriptor box and records that explicit cleanup survived the contained panic.
unsafe extern "C" fn release(context: *mut c_void, value: *mut c_void) -> i32 {
    unsafe { drop(Box::from_raw(value.cast::<MbCoercionInputV1>())); *context.cast::<usize>() -= 1; }
    0
}

/// Rejects unexpected array iteration in this exact-string panic regression.
unsafe extern "C" fn next(
    _context: *mut c_void, _array: *const MbHostValueV1, _cursor: *mut u64,
    _key: *mut MbHostValueV1, _value: *mut MbHostValueV1,
) -> u64 { ITER_ERROR }

/// Contains an operation panic, cleans all previously cloned native arguments, and remains reusable.
#[test]
fn mbstring_invoke_cleans_owners_after_rust_panic() {
    let mut live = 0_usize;
    let host = MbInvokeHostV1 { version: 1, size: std::mem::size_of::<MbInvokeHostV1>() as u32,
        context: (&mut live as *mut usize).cast(), clone_value: Some(clone_value), describe_value: Some(describe),
        stringable: Some(stringable), format_float: Some(float), diagnostic: Some(diagnostic),
        release_owner: Some(release), array_next: Some(next) };
    let input = MbCoercionInputV1 { kind: HOST_STRING, value: 0, bytes: b"abc".as_ptr(), len: 3, flags: 0 };
    let args = [(&input as *const MbCoercionInputV1).cast::<c_void>()];
    let mut output = MbResultV1::default();
    REQUEST.with(|state| {
        let _borrow = state.borrow_mut();
        let status = unsafe { elephc_mbstring_invoke_v1(RuntimeBuiltinId::MbStrlen.as_u32(), args.as_ptr(), 1, 0, &host, &mut output) };
        assert_eq!(status, RuntimeBuiltinStatus::RuntimeFatal as i32);
        assert_eq!(output.kind, RESULT_FATAL);
        assert_eq!(live, 0);
    });
    unsafe { elephc_mbstring_release_v1(&mut output); }
    let status = unsafe { elephc_mbstring_invoke_v1(RuntimeBuiltinId::MbStrlen.as_u32(), args.as_ptr(), 1, 0, &host, &mut output) };
    assert_eq!(status, RuntimeBuiltinStatus::Success as i32);
    assert_eq!(output.kind, RESULT_INT);
    assert_eq!(output.value, 3);
    assert_eq!(live, 0);
    unsafe { elephc_mbstring_release_v1(&mut output); }
}

/// Rejects list coercion because the panic fixture only snapshots a direct array parameter.
unsafe extern "C" fn array_value(
    _context: *mut c_void, _source: *const MbArraySourceV2, _cursor: *mut u64, _out: *mut MbArrayEntryV2,
) -> i32 { 1 }

/// Publishes one owned scalar and identity lease before the request dispatch deliberately panics.
unsafe extern "C" fn graph_value(
    context: *mut c_void, _source: *const MbArraySourceV2, cursor: *mut u64, out: *mut MbArrayEntryV3,
) -> i32 {
    if unsafe { *cursor } != 0 { return 0; }
    let value = MbCoercionInputV1 { kind: HOST_STRING, value: 0, bytes: b"ok".as_ptr(), len: 2, flags: 0 };
    unsafe {
        clone_value(context, (&value as *const MbCoercionInputV1).cast(), &mut (*out).owner);
        clone_value(context, (&value as *const MbCoercionInputV1).cast(), &mut (*out).original);
        (*out).key = MbHostValueV1 { tag: HOST_INT, lo: 0, hi: 0 };
        (*out).kind = ITER_ENTRY;
        *cursor = 1;
    }
    0
}

/// Contains a dispatch panic after recursive snapshotting and consumes all graph and argument pins.
#[test]
fn mbstring_invoke_v3_cleans_graph_after_rust_panic() {
    let mut live = 0_usize;
    let base = MbInvokeHostV1 { version: 3, size: std::mem::size_of::<MbInvokeHostV3>() as u32,
        context: (&mut live as *mut usize).cast(), clone_value: Some(clone_value), describe_value: Some(describe),
        stringable: Some(stringable), format_float: Some(float), diagnostic: Some(diagnostic),
        release_owner: Some(release), array_next: Some(next) };
    let host = MbInvokeHostV3 { base: MbInvokeHostV2 { base, array_value: Some(array_value) },
        graph_value: Some(graph_value), pin_value: Some(clone_value) };
    let input = MbCoercionInputV1 { kind: HOST_INDEXED_ARRAY, value: 1, bytes: std::ptr::null(), len: 0, flags: 0 };
    let args = [(&input as *const MbCoercionInputV1).cast::<c_void>()];
    let mut output = MbResultV1::default();
    REQUEST.with(|state| {
        let _borrow = state.borrow_mut();
        let status = unsafe { elephc_mbstring_invoke_v1(RuntimeBuiltinId::MbCheckEncoding.as_u32(),
            args.as_ptr(), 1, 0, &host.base.base, &mut output) };
        assert_eq!(status, 1);
        assert_eq!(output.kind, RESULT_FATAL);
        assert_eq!(live, 0);
    });
    unsafe { elephc_mbstring_release_v1(&mut output); }
    let status = unsafe { elephc_mbstring_invoke_v1(RuntimeBuiltinId::MbCheckEncoding.as_u32(),
        args.as_ptr(), 1, 0, &host.base.base, &mut output) };
    assert_eq!(status, 0);
    assert_eq!(output.kind, RESULT_BOOL);
    assert_eq!(output.value, 1);
    assert_eq!(live, 0);
    unsafe { elephc_mbstring_release_v1(&mut output); }
}

/// Publishes a fresh null writer so the panic test observes a separately owned construction resource.
unsafe extern "C" fn capture_initialize(context: *mut c_void, _: *const c_void, out: *mut MbCaptureOutputV1) -> i32 {
    let value = MbCoercionInputV1 { kind: HOST_NULL, value: 0, bytes: std::ptr::null(), len: 0, flags: 0 };
    unsafe {
        clone_value(context, (&value as *const MbCoercionInputV1).cast(), &mut (*out).writer);
        (*out).ready = 1;
    }
    0
}

/// Rejects filling because this fixture panics immediately after acquiring its construction writer.
unsafe extern "C" fn capture_fill(_: *mut c_void, _: *mut c_void, _: *const u8, _: u64) -> i32 { 1 }

/// Contains a Rust panic after output initialization and consumes the published writer exactly once.
#[test]
fn mbstring_invoke_v4_cleans_capture_after_rust_panic() {
    let mut live = 0_usize;
    let base = MbInvokeHostV1 { version: 4, size: std::mem::size_of::<MbInvokeHostV4>() as u32,
        context: (&mut live as *mut usize).cast(), clone_value: Some(clone_value), describe_value: Some(describe),
        stringable: Some(stringable), format_float: Some(float), diagnostic: Some(diagnostic),
        release_owner: Some(release), array_next: Some(next) };
    let host = MbInvokeHostV4 { base: MbInvokeHostV3 {
        base: MbInvokeHostV2 { base, array_value: Some(array_value) }, graph_value: Some(graph_value), pin_value: Some(clone_value),
    }, capture_initialize: Some(capture_initialize), capture_fill: Some(capture_fill), capture_release: Some(release) };
    let mut output = MbResultV1::default();
    let status = unsafe { boundary(&mut output, |session| {
        session.initialize(&host.base.base.base, 3)?;
        assert_eq!(session.initialize_capture(2), (true, None));
        assert_eq!(live, 1);
        panic!("contained failure after output initialization");
    }) };
    assert_eq!(status, RuntimeBuiltinStatus::RuntimeFatal as i32);
    assert_eq!(live, 0);
    unsafe { elephc_mbstring_release_v1(&mut output); }
}
