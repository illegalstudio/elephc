//! Purpose:
//! Exercises native INI ownership, protected diagnostics, process configuration, and request isolation.
//!
//! Called from:
//! - The standalone mbstring INI ABI integration binary.
//!
//! Key details:
//! - One test owns process-global provider/configuration installation and tests malformed responses.
//! - Existing text operation calls prove that INI updates reach the same request state.

use std::{ffi::c_void, sync::atomic::{AtomicUsize, Ordering}};
use elephc_builtin_contract::{RuntimeBuiltinId, mbstring_abi::{*, ini::*}};
use elephc_mbstring::abi::*;

static HANDLES: AtomicUsize = AtomicUsize::new(0);
static COMPILES: AtomicUsize = AtomicUsize::new(0);

/// Copies public result values, validates INI identity framing, and releases all result ownership.
fn result(status: i32, mut value: MbResultV1) -> (i32, u64, i64, Vec<u8>) {
    let bytes = if value.len == 0 { Vec::new() } else { unsafe { std::slice::from_raw_parts(value.bytes, value.len as usize).to_vec() } };
    let output = if value.kind == RESULT_INI_STRING { (status, RESULT_STRING, 0, bytes) }
        else if value.kind == RESULT_INI_ARRAY {
            let (graph, _) = decode_ini_array(&bytes, value.value as u64).unwrap();
            (status, RESULT_ARRAY, 0, graph.encode())
        } else { (status, value.kind, value.value, bytes) };
    unsafe { elephc_mbstring_release_v1(&mut value); elephc_mbstring_release_v1(&mut value); }
    output
}

/// Runs an ordinary text operation to observe the native bridge's existing request state.
fn text(operation: RuntimeBuiltinId, args: &[MbArgV1]) -> (i32, u64, i64, Vec<u8>) {
    let mut output = MbResultV1::default();
    unsafe { elephc_mbstring_call_v1(operation.as_u32(), args.as_ptr(), args.len() as u64, &mut output); }
    result(0, output)
}

/// Builds a complete short-lived host table with optional test callback state.
fn host(context: *mut c_void) -> MbIniHostV1 {
    MbIniHostV1 { version: 1, size: std::mem::size_of::<MbIniHostV1>() as u32, context, diagnostic: Some(diagnostic) }
}

/// Invokes an INI operation through its actual C ABI and releases its result ownership.
fn ini(operation: u32, args: &[MbArgV1], context: *mut c_void) -> (i32, u64, i64, Vec<u8>) {
    let mut output = MbResultV1::default();
    let status = unsafe { elephc_mbstring_ini_v1(operation, args.as_ptr(), args.len() as u64, &host(context), &mut output) };
    result(status, output)
}

/// Applies process startup configuration through the C ABI, preserving diagnostic order.
fn configure(args: &[MbArgV1], context: *mut c_void) -> (i32, u64, i64, Vec<u8>) {
    let mut output = MbResultV1::default();
    let status = unsafe { elephc_mbstring_configure_v1(args.as_ptr(), args.len() as u64, &host(context), &mut output) };
    result(status, output)
}

/// Reads raw text through an independent nested INI call.
fn raw(name: &[u8]) -> Vec<u8> { ini(INI_GET, &[MbArgV1::string(name)], std::ptr::null_mut()).3 }

/// Changes raw INI text with a silent protected diagnostic sink.
fn set(name: &[u8], value: &[u8]) -> (i32, u64, i64, Vec<u8>) {
    ini(INI_SET, &[MbArgV1::string(name), MbArgV1::string(value)], std::ptr::null_mut())
}

/// Tracks emitted warnings and the raw/effective state seen at the callback boundary.
#[derive(Default)]
struct Context { mode: u8, events: Vec<(u32, Vec<u8>, Vec<u8>, Vec<u8>)> }

/// Observes state, optionally reenters INI, and returns a protected pending/fatal status without unwinding.
unsafe extern "C" fn diagnostic(context: *mut c_void, level: u32, bytes: *const u8, length: u64) -> i32 {
    if context.is_null() { return 0; }
    let context = unsafe { &mut *context.cast::<Context>() };
    let message = if length == 0 { Vec::new() } else { unsafe { std::slice::from_raw_parts(bytes, length as usize).to_vec() } };
    context.events.push((level, message, raw(b"mbstring.internal_encoding"), text(RuntimeBuiltinId::MbInternalEncoding, &[]).3));
    if context.mode == 1 { set(b"mbstring.internal_encoding", b"ISO-8859-1"); }
    if context.mode == 2 { 2 } else if context.mode == 3 { 47 } else { 0 }
}

/// Supplies valid, syntax-error, and malformed responses to test provider ownership and failure handling.
unsafe extern "C" fn compile(handle: *mut *mut c_void, bytes: *const u8, length: u64, offset: *mut u64) -> i32 {
    COMPILES.fetch_add(1, Ordering::SeqCst);
    let pattern = unsafe { std::slice::from_raw_parts(bytes, length as usize) };
    unsafe { *handle = std::ptr::null_mut(); *offset = 0; }
    if pattern == b"bad-success" { return 0; }
    if pattern == b"[" { unsafe { *offset = 1; } return 106; }
    HANDLES.fetch_add(1, Ordering::SeqCst);
    unsafe { *handle = Box::into_raw(Box::new(7_u8)).cast(); }
    if pattern == b"bad-error-owner" { 106 } else { 0 }
}

/// Reports a nonmatch for the provider-table fixture, which tests validation rather than matching syntax.
unsafe extern "C" fn matches(_handle: *mut c_void, _bytes: *const u8, _length: u64) -> i32 { 0 }

/// Releases the exact allocation created by the compile fixture, including malformed error responses.
unsafe extern "C" fn free(handle: *mut c_void) {
    if !handle.is_null() { unsafe { drop(Box::from_raw(handle.cast::<u8>())); } HANDLES.fetch_sub(1, Ordering::SeqCst); }
}

/// Returns the known native compile message while obeying the provider's NUL-termination contract.
unsafe extern "C" fn error(_code: i32, buffer: *mut u8, capacity: u64) -> i32 {
    let message = b"missing terminating ] for character class\0";
    if capacity < message.len() as u64 { return -1; }
    unsafe { std::ptr::copy_nonoverlapping(message.as_ptr(), buffer, message.len()); }
    message.len() as i32 - 1
}

/// Checks callback-time state, pending exceptions, safe native failures, and immutable startup inheritance.
#[test]
fn ini_abi_owns_results_preserves_reentry_and_resets_workers() {
    let null = std::ptr::null_mut();
    let internal = b"mbstring.internal_encoding";
    let mime = b"mbstring.http_output_conv_mimetypes";
    elephc_mbstring_reset_v1();
    assert_eq!(raw(internal), b"");
    assert_eq!(ini(INI_GET, &[MbArgV1::string(b"missing")], null), (0, RESULT_BOOL, 0, vec![]));
    assert_eq!(ini(INI_GET, &[], null).0, 1);
    assert_eq!(ini(INI_SET, &[MbArgV1::string(internal), MbArgV1::integer(1)], null).0, 1);
    let mut output = MbResultV1::default();
    assert_eq!(unsafe { elephc_mbstring_ini_v1(INI_GET, std::ptr::null(), u64::MAX, &host(null), &mut output) }, 1);
    result(1, output);
    assert_eq!(set(mime, b"["), (1, RESULT_FATAL, 0, vec![]), "missing provider must fail without a fabricated PHP warning");

    let provider = MbMimeRegexV1 { version: 1, size: std::mem::size_of::<MbMimeRegexV1>() as u32,
        compile: Some(compile), matches: Some(matches), free: Some(free), error: Some(error) };
    assert_eq!(unsafe { elephc_mbstring_mime_provider_v1(&MbMimeRegexV1 { version: 2, ..provider }) }, 1);
    assert_eq!(unsafe { elephc_mbstring_mime_provider_v1(&provider) }, 0);
    assert_eq!(unsafe { elephc_mbstring_mime_provider_v1(&provider) }, 0);
    let mut context = Context { mode: 1, ..Context::default() };
    let pointer = (&mut context as *mut Context).cast();
    assert_eq!(ini(INI_SET, &[MbArgV1::string(internal), MbArgV1::string(b"SJIS")], pointer), (0, RESULT_STRING, 0, vec![]));
    assert_eq!(context.events[0].2, b"");
    assert_eq!(context.events[0].3, b"UTF-8");
    assert_eq!(raw(internal), b"ISO-8859-1");
    assert_eq!(text(RuntimeBuiltinId::MbInternalEncoding, &[]).3, b"SJIS");

    context.mode = 2;
    context.events.clear();
    assert_eq!(ini(INI_SET, &[MbArgV1::string(internal), MbArgV1::string(b"bogus")], pointer), (2, RESULT_FATAL, 0, vec![]));
    assert_eq!(context.events.len(), 1, "later diagnostics must not reenter a throwing PHP handler");
    assert_eq!(raw(internal), b"bogus");
    assert_eq!(text(RuntimeBuiltinId::MbInternalEncoding, &[]).3, b"UTF-8");
    context.mode = 0;
    context.events.clear();
    assert_eq!(ini(INI_SET, &[MbArgV1::string(mime), MbArgV1::string(b"[")], pointer), (0, RESULT_BOOL, 0, vec![]));
    assert_eq!(context.events[0].1, b"ini_set(): [ (offset=1): missing terminating ] for character class");
    for invalid in [b"bad-success".as_slice(), b"bad-error-owner"] { assert_eq!(set(mime, invalid).0, 1); }
    assert_eq!(HANDLES.load(Ordering::SeqCst), 0);

    let configuration = [MbArgV1::string(b"UTF-8"), MbArgV1::string(b"UTF-8"), MbArgV1::string(b"UTF-8"),
        MbArgV1::string(b"mbstring.language"), MbArgV1::string(b"Japanese"),
        MbArgV1::string(b"mbstring.http_input"), MbArgV1::string(b"ASCII,SJIS")];
    assert_eq!(configure(&configuration, null).0, 0);
    assert_eq!(text(RuntimeBuiltinId::MbLanguage, &[]).3, b"Japanese");
    assert_eq!(raw(internal), b"");
    text(RuntimeBuiltinId::MbLanguage, &[MbArgV1::string(b"Korean")]);
    let calls = COMPILES.load(Ordering::SeqCst);
    assert_eq!(configure(&configuration, null).0, 0);
    assert_eq!(text(RuntimeBuiltinId::MbLanguage, &[]).3, b"Korean", "repeat initialization cannot reset a running request");
    assert_eq!(COMPILES.load(Ordering::SeqCst), calls);
    assert_eq!(configure(&configuration[..3], null).0, 1, "different process defaults must fail closed");
    text(RuntimeBuiltinId::MbScrub, &[MbArgV1::string(b"\xff"), MbArgV1::string(b"UTF-8")]);
    elephc_mbstring_reset_v1();
    assert_eq!(text(RuntimeBuiltinId::MbGetInfo, &[MbArgV1::string(b"illegal_chars")]).2, 0);
    assert_eq!(text(RuntimeBuiltinId::MbLanguage, &[]).3, b"Japanese");
    assert_eq!(COMPILES.load(Ordering::SeqCst), calls, "request reset must reuse validated MIME configuration");
    let worker = std::thread::spawn(|| {
        assert_eq!(text(RuntimeBuiltinId::MbLanguage, &[]).3, b"Japanese");
        assert_eq!(text(RuntimeBuiltinId::MbHttpInput, &[MbArgV1::string(b"L")]).3, b"ASCII,SJIS");
        text(RuntimeBuiltinId::MbLanguage, &[MbArgV1::string(b"Korean")]);
        elephc_mbstring_reset_v1();
        text(RuntimeBuiltinId::MbLanguage, &[]).3
    }).join().unwrap();
    assert_eq!(worker, b"Japanese");
    assert_eq!(text(RuntimeBuiltinId::MbLanguage, &[]).3, b"Japanese");
    context.events.clear();
    context.mode = 1;
    let core = [MbArgV1::string(b"bogus"), MbArgV1::string(b"UTF-16LE"), MbArgV1::string(b"ISO-8859-1")];
    let mut output = MbResultV1::default();
    let status = unsafe { elephc_mbstring_core_encoding_v1(core.as_ptr(), 3, &host(pointer), &mut output) };
    assert_eq!(result(status, output).0, 0);
    assert_eq!(context.events.len(), 1);
    assert_eq!(raw(internal), b"ISO-8859-1");
    assert_eq!(text(RuntimeBuiltinId::MbInternalEncoding, &[]).3, b"UTF-8");
    assert_eq!(text(RuntimeBuiltinId::MbHttpOutput, &[]).3, b"ISO-8859-1");
    assert_eq!(text(RuntimeBuiltinId::MbHttpInput, &[MbArgV1::string(b"L")]).3, b"ASCII,SJIS");
    elephc_mbstring_reset_v1();
    assert_eq!(raw(internal), b"");
    assert_eq!(text(RuntimeBuiltinId::MbHttpOutput, &[]).3, b"UTF-8");
    let all = ini(INI_GET_ALL, &[MbArgV1::boolean(true)], null);
    assert_eq!(all.1, RESULT_ARRAY);
    assert_eq!(array::ArrayGraph::decode(&all.3).unwrap().arrays()[0].len(), 11);
    assert_eq!(HANDLES.load(Ordering::SeqCst), 0);
}
