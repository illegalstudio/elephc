//! Purpose:
//! Checks mbregex configuration, warning reentry, and worker lifetime through the exported C ABI.
//!
//! Called from:
//! - The focused mbstring regex/INI integration test binary.
//!
//! Key details:
//! - One test owns process configuration; independent worker threads inherit the same startup alias.
//! - Settings require no Oniguruma provider and an empty MIME pattern avoids unrelated native dependencies.

use std::ffi::c_void;
use elephc_builtin_contract::{RuntimeBuiltinId, mbstring_abi::{*, ini::*}};
use elephc_mbstring::abi::*;

/// Copies the bridge result and releases its owned payload without requiring UTF-8.
fn release(mut result: MbResultV1) -> (u64, i64, Vec<u8>) {
    let bytes = if result.len == 0 { Vec::new() } else { unsafe { std::slice::from_raw_parts(result.bytes, result.len as usize).to_vec() } };
    let output = (result.kind, result.value, bytes);
    unsafe { elephc_mbstring_release_v1(&mut result); }
    output
}

/// Runs one already coerced public setting through the same ABI used by AOT and eval.
fn call(operation: RuntimeBuiltinId, value: Option<&[u8]>) -> (u64, i64, Vec<u8>) {
    let arguments: Vec<_> = value.map(MbArgV1::string).into_iter().collect();
    let mut result = MbResultV1::default();
    unsafe { elephc_mbstring_call_v1(operation.as_u32(), arguments.as_ptr(), arguments.len() as u64, &mut result); }
    release(result)
}

/// Records the text and regex settings observed by each protected warning callback.
#[derive(Default)]
struct Context { mode: u8, events: Vec<(u32, Vec<u8>, Vec<u8>)> }

/// Reenters regex settings outside request borrows and optionally reports a pending PHP exception.
unsafe extern "C" fn diagnostic(context: *mut c_void, level: u32, _bytes: *const u8, _length: u64) -> i32 {
    if context.is_null() { return 0; }
    let context = unsafe { &mut *context.cast::<Context>() };
    context.events.push((level, call(RuntimeBuiltinId::MbInternalEncoding, None).2,
        call(RuntimeBuiltinId::MbRegexEncoding, None).2));
    if context.mode != 0 { call(RuntimeBuiltinId::MbRegexEncoding, Some(b"ISO-8859-1")); }
    if context.mode == 2 { 2 } else { 0 }
}

/// Supplies a complete callback table for one synchronous native INI operation.
fn host(context: *mut c_void) -> MbIniHostV1 {
    MbIniHostV1 { version: 1, size: std::mem::size_of::<MbIniHostV1>() as u32, context, diagnostic: Some(diagnostic) }
}

/// Invokes a native INI operation and releases its returned raw-string lease or empty result.
fn ini(operation: u32, arguments: &[MbArgV1], context: *mut c_void) -> i32 {
    let mut result = MbResultV1::default();
    let status = unsafe { elephc_mbstring_ini_v1(operation, arguments.as_ptr(), arguments.len() as u64, &host(context), &mut result) };
    release(result);
    status
}

/// Preserves PHP handler ordering, invalid-name semantics, option persistence, and thread isolation.
#[test]
fn regex_ini_abi_configuration_reentry_and_workers() {
    let null = std::ptr::null_mut();
    let arguments = [MbArgV1::string(b"SJIS-WIN"), MbArgV1::string(b"UTF-8"), MbArgV1::string(b"UTF-8"),
        MbArgV1::string(b"mbstring.http_output_conv_mimetypes"), MbArgV1::string(b"")];
    let mut result = MbResultV1::default();
    assert_eq!(unsafe { elephc_mbstring_configure_v1(arguments.as_ptr(), arguments.len() as u64, &host(null), &mut result) }, 0);
    release(result);
    assert_eq!(call(RuntimeBuiltinId::MbRegexEncoding, None).2, b"SJIS");
    assert_eq!(call(RuntimeBuiltinId::MbRegexSetOptions, None).2, b"pr");
    call(RuntimeBuiltinId::MbInternalEncoding, Some(b"ASCII"));
    assert_eq!(call(RuntimeBuiltinId::MbRegexEncoding, None).2, b"SJIS");
    call(RuntimeBuiltinId::MbRegexEncoding, Some(b"UTF-16LE"));
    call(RuntimeBuiltinId::MbRegexSetOptions, Some(b"ixm"));

    let name = MbArgV1::string(b"mbstring.internal_encoding");
    let mut context = Context { mode: 1, ..Context::default() };
    let pointer = (&mut context as *mut Context).cast();
    assert_eq!(ini(INI_SET, &[name, MbArgV1::string(b"ASCII")], pointer), 0);
    assert_eq!(context.events, vec![(8192, b"ASCII".to_vec(), b"UTF-16LE".to_vec())]);
    assert_eq!(call(RuntimeBuiltinId::MbRegexEncoding, None).2, b"ASCII", "the outer valid handler commits after warning reentry");
    context.events.clear();
    assert_eq!(ini(INI_SET, &[name, MbArgV1::string(b"invalid")], pointer), 0);
    assert_eq!(context.events, vec![(8192, b"ASCII".to_vec(), b"ASCII".to_vec()),
        (2, b"ASCII".to_vec(), b"ISO-8859-1".to_vec())]);
    assert_eq!(call(RuntimeBuiltinId::MbInternalEncoding, None).2, b"UTF-8");
    assert_eq!(call(RuntimeBuiltinId::MbRegexEncoding, None).2, b"ISO-8859-1", "unsupported regex names retain a callback's current encoding");
    context.events.clear();
    context.mode = 2;
    assert_eq!(ini(INI_SET, &[name, MbArgV1::string(b"UTF-32LE")], pointer), 2);
    assert_eq!(context.events.len(), 1);
    assert_eq!(call(RuntimeBuiltinId::MbRegexEncoding, None).2, b"UCS-4LE", "pending exceptions do not skip the INI handler's commit");
    assert_eq!(ini(INI_RESTORE, &[name], null), 0);
    assert_eq!(call(RuntimeBuiltinId::MbRegexEncoding, None).2, b"SJIS");
    assert_eq!(ini(INI_SET, &[name, MbArgV1::string(b"ASCII")], null), 0);
    elephc_mbstring_reset_v1();
    assert_eq!(call(RuntimeBuiltinId::MbRegexEncoding, None).2, b"SJIS", "reset restores startup INI defaults before regex shutdown");
    assert_eq!(call(RuntimeBuiltinId::MbRegexSetOptions, None).2, b"ixmr");

    context.mode = 0;
    context.events.clear();
    let core = [MbArgV1::string(b"UTF-16LE"), MbArgV1::string(b"invalid"), MbArgV1::string(b"UTF-8")];
    let mut result = MbResultV1::default();
    assert_eq!(unsafe { elephc_mbstring_core_encoding_v1(core.as_ptr(), 3, &host(pointer), &mut result) }, 0);
    release(result);
    assert_eq!(context.events, vec![(2, b"UTF-16LE".to_vec(), b"UTF-16LE".to_vec())],
        "later input warnings see the completed internal/regex encoding handler");
    call(RuntimeBuiltinId::MbInternalEncoding, Some(b"ASCII"));
    context.events.clear();
    let core = [MbArgV1::string(b"UTF-32LE"), MbArgV1::string(b"UTF-8"), MbArgV1::string(b"UTF-8")];
    let mut result = MbResultV1::default();
    assert_eq!(unsafe { elephc_mbstring_core_encoding_v1(core.as_ptr(), 3, &host(pointer), &mut result) }, 0);
    release(result);
    assert!(context.events.is_empty());
    assert_eq!(call(RuntimeBuiltinId::MbInternalEncoding, None).2, b"ASCII");
    assert_eq!(call(RuntimeBuiltinId::MbRegexEncoding, None).2, b"UTF-16LE",
        "an explicit text setter prevents inherited core encoding updates");

    std::thread::spawn(|| {
        assert_eq!(call(RuntimeBuiltinId::MbRegexEncoding, None).2, b"SJIS");
        assert_eq!(call(RuntimeBuiltinId::MbRegexSetOptions, None).2, b"pr");
        call(RuntimeBuiltinId::MbRegexEncoding, Some(b"UTF-8"));
        call(RuntimeBuiltinId::MbRegexSetOptions, Some(b"j"));
        elephc_mbstring_reset_v1();
        assert_eq!(call(RuntimeBuiltinId::MbRegexEncoding, None).2, b"SJIS");
        assert_eq!(call(RuntimeBuiltinId::MbRegexSetOptions, None).2, b"j");
    }).join().unwrap();
    assert_eq!(call(RuntimeBuiltinId::MbRegexSetOptions, None).2, b"ixmr");
}
