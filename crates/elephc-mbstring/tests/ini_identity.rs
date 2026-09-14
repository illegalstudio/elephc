//! Purpose:
//! Checks INI string aliases, graph metadata, and native lease cleanup through the public bridge ABI.
//!
//! Called from:
//! - The standalone mbstring identity integration binary.
//!
//! Key details:
//! - Warning callbacks reenter the actual request without holding host or Rust state borrows.
//! - Tests distinguish equal bytes, one-byte getter normalization, expired leases, and thread resets.

use std::ffi::c_void;
use elephc_builtin_contract::mbstring_abi::{*, ini::*};
use elephc_mbstring::abi::*;

static TESTS: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Owns every byte payload and identity lease returned by one successful or failed INI call.
struct Owned(MbResultV1);

impl Owned {
    /// Borrows a result's ordinary string bytes or complete graph wire payload.
    fn bytes(&self) -> &[u8] {
        if self.0.len == 0 { &[] } else { unsafe { std::slice::from_raw_parts(self.0.bytes, self.0.len as usize) } }
    }
    /// Exposes a live owned INI string as a borrowed setter argument.
    fn argument(&self) -> MbArgV1 { assert_eq!(self.0.kind, RESULT_INI_STRING); string_argument(self.0.value as u64) }
}

impl Drop for Owned {
    /// Releases both buffers and leases, then confirms empty-result cleanup is idempotent.
    fn drop(&mut self) { unsafe { elephc_mbstring_release_v1(&mut self.0); elephc_mbstring_release_v1(&mut self.0); } }
}

/// Invokes the dedicated INI protocol with an optional protected warning context.
fn call(op: u32, args: &[MbArgV1], context: *mut c_void) -> (i32, Owned) {
    let mut result = MbResultV1::default();
    let host = MbIniHostV1 { version: 1, size: std::mem::size_of::<MbIniHostV1>() as u32, context, diagnostic: Some(diagnostic) };
    let status = unsafe { elephc_mbstring_ini_v1(op, args.as_ptr(), args.len() as u64, &host, &mut result) };
    (status, Owned(result))
}

/// Executes a valid silent operation and keeps its complete result ownership available to the caller.
fn ok(op: u32, args: &[MbArgV1]) -> Owned {
    let (status, result) = call(op, args, std::ptr::null_mut());
    assert_eq!(status, 0);
    result
}

/// Records callback mode and observed identities without throwing across the C boundary.
struct Context { key: &'static [u8], mode: u8, observed: Vec<u64>, failures: Vec<i32> }

/// Reuses a scalar getter, raw array cell, equal fresh copy, or interned literal in a nested write.
unsafe extern "C" fn diagnostic(context: *mut c_void, _level: u32, _bytes: *const u8, _length: u64) -> i32 {
    if context.is_null() { return 0; }
    let context = unsafe { &mut *context.cast::<Context>() };
    let key = MbArgV1::string(context.key);
    let (status, value) = match context.mode {
        1 => call(INI_GET_ALL, &[MbArgV1::boolean(false)], std::ptr::null_mut()),
        2 => call(INI_STRING_NEW, &[MbArgV1::string(b"ASCII")], std::ptr::null_mut()),
        3 => call(INI_STRING_INTERNED, &[MbArgV1::string(b"ASCII")], std::ptr::null_mut()),
        _ => call(INI_GET, &[key], std::ptr::null_mut()),
    };
    context.failures.push(status);
    let identity = if context.mode == 1 {
        let Some((graph, identities)) = decode_ini_array(value.bytes(), value.0.value as u64) else { return 1; };
        let Some(entry) = graph.arrays()[0].iter().position(|(key, _)| matches!(key, array::Key::String(bytes) if bytes == context.key)) else { return 1; };
        let Some((_, _, identity)) = identities.iter().find(|(array, slot, _)| *array == 0 && *slot == entry) else { return 1; };
        *identity
    } else { value.0.value as u64 };
    context.observed.push(identity);
    context.failures.push(call(INI_SET, &[key, string_argument(identity)], std::ptr::null_mut()).0);
    0
}

/// Distinguishes aliased getters, equal copies, and one-byte normalization at warning-time commit guards.
#[test]
fn ini_abi_preserves_string_identity_during_warning_reentry() {
    let _serial = TESTS.lock().unwrap();
    for (key, old, changed, mode, interned, expected) in [
        (b"mbstring.internal_encoding".as_slice(), b"ASCII".as_slice(), b"SJIS".as_slice(), 0, false, b"SJIS".as_slice()),
        (b"mbstring.internal_encoding", b"ASCII", b"SJIS", 1, false, b"SJIS"),
        (b"mbstring.internal_encoding", b"ASCII", b"SJIS", 2, false, b"ASCII"),
        (b"mbstring.internal_encoding", b"ASCII", b"SJIS", 3, true, b"SJIS"),
        (b"mbstring.internal_encoding", b"ASCII", b"SJIS", 3, false, b"ASCII"),
        (b"mbstring.regex_stack_limit", b"3", b"1badK", 0, false, b"3"),
        (b"mbstring.regex_stack_limit", b"3", b"1badK", 1, false, b"1badK"),
        (b"mbstring.regex_stack_limit", b"", b"1badK", 0, false, b""),
        (b"mbstring.regex_stack_limit", b"", b"1badK", 1, false, b"1badK"),
    ] {
        elephc_mbstring_reset_v1();
        let original = ok(if interned { INI_STRING_INTERNED } else { INI_STRING_NEW }, &[MbArgV1::string(old)]);
        let name = MbArgV1::string(key);
        ok(INI_SET, &[name, original.argument()]);
        let mut context = Context { key, mode, observed: Vec::new(), failures: Vec::new() };
        let (status, previous) = call(INI_SET, &[name, MbArgV1::string(changed)], (&mut context as *mut Context).cast());
        assert_eq!(status, 0);
        assert_eq!(previous.bytes(), old);
        assert_eq!(context.failures, [0, 0]);
        assert_eq!(ok(INI_GET, &[name]).bytes(), expected, "key {}, mode {mode}, old {old:?}", String::from_utf8_lossy(key));
        if expected == changed { assert_eq!(context.observed, [original.0.value as u64]); }
        else { assert_ne!(context.observed, [original.0.value as u64]); }
    }
}

/// Verifies lease expiry, fresh identity allocation, graph ownership, and retained values across request resets.
#[test]
fn ini_identity_leases_end_with_their_last_host_owner() {
    let _serial = TESTS.lock().unwrap();
    let original = ok(INI_STRING_NEW, &[MbArgV1::string(b"ASCII")]);
    let identity = original.0.value as u64;
    assert_eq!(elephc_mbstring_ini_string_retain_v1(identity), 0);
    drop(original);
    let name = MbArgV1::string(b"mbstring.internal_encoding");
    ok(INI_SET, &[name, string_argument(identity)]);
    elephc_mbstring_reset_v1();
    std::thread::spawn(move || {
        ok(INI_SET, &[MbArgV1::string(b"mbstring.internal_encoding"), string_argument(identity)]);
        assert_eq!(ok(INI_GET, &[MbArgV1::string(b"mbstring.internal_encoding")]).bytes(), b"ASCII");
        elephc_mbstring_reset_v1();
    }).join().unwrap();
    assert_eq!(elephc_mbstring_ini_string_release_v1(identity), 0);
    assert_eq!(elephc_mbstring_ini_string_retain_v1(identity), 1);
    assert_eq!(elephc_mbstring_ini_string_release_v1(identity), 1);
    assert_eq!(call(INI_SET, &[name, string_argument(identity)], std::ptr::null_mut()).0, 1);
    let replacement = ok(INI_STRING_NEW, &[MbArgV1::string(b"ASCII")]);
    assert_ne!(replacement.0.value as u64, identity);
    for details in [false, true] {
        let result = ok(INI_GET_ALL, &[MbArgV1::boolean(details)]);
        assert_eq!(result.0.kind, RESULT_INI_ARRAY);
        let (_, records) = decode_ini_array(result.bytes(), result.0.value as u64).unwrap();
        assert!(!records.is_empty());
        let ids: std::collections::HashSet<_> = records.into_iter().map(|(_, _, id)| id).collect();
        for &id in &ids { assert_eq!(elephc_mbstring_ini_string_retain_v1(id), 0); }
        drop(result);
        for id in ids {
            assert_eq!(elephc_mbstring_ini_string_release_v1(id), 0);
            assert_eq!(elephc_mbstring_ini_string_retain_v1(id), 1);
        }
    }
}
