//! Purpose:
//! Connects the real embedded native PCRE2 shim to the mbstring INI bridge.
//!
//! Called from:
//! - The host integration harness on supported Unix compiler hosts.
//!
//! Key details:
//! - Compiles against one aligned host provider, with missing development files fatal.
//! - The loaded provider intentionally remains resident for its process-lifetime callback contract.

use std::ffi::c_void;
use elephc_builtin_contract::mbstring_abi::{*, ini::*};
use elephc_mbstring::abi::*;

#[path = "support/mime_provider.rs"]
mod mime_provider;
use mime_provider::provider;

/// Copies one warning through a protected host callback without invoking PHP or unwinding.
unsafe extern "C" fn diagnostic(context: *mut c_void, level: u32, bytes: *const u8, length: u64) -> i32 {
    let warnings = unsafe { &mut *context.cast::<Vec<(u32, Vec<u8>)>>() };
    let text = if length == 0 { Vec::new() } else { unsafe { std::slice::from_raw_parts(bytes, length as usize).to_vec() } };
    warnings.push((level, text));
    0
}

/// Runs a coerced INI call and consumes its owned result after copying the byte payload.
fn call(op: u32, args: &[MbArgV1], warnings: &mut Vec<(u32, Vec<u8>)>) -> (i32, u64, i64, Vec<u8>) {
    let host = MbIniHostV1 { version: 1, size: std::mem::size_of::<MbIniHostV1>() as u32,
        context: (warnings as *mut Vec<(u32, Vec<u8>)>).cast(), diagnostic: Some(diagnostic) };
    let mut result = MbResultV1::default();
    let status = unsafe { elephc_mbstring_ini_v1(op, args.as_ptr(), args.len() as u64, &host, &mut result) };
    let bytes = if result.len == 0 { Vec::new() } else { unsafe { std::slice::from_raw_parts(result.bytes, result.len as usize).to_vec() } };
    let output = if result.kind == RESULT_INI_STRING { (status, RESULT_STRING, 0, bytes) }
        else { (status, result.kind, result.value, bytes) };
    unsafe { elephc_mbstring_release_v1(&mut result); }
    output
}

/// Preserves PHP trimming, C-string boundaries, exact native errors, and old raw text through real PCRE2.
#[test]
fn ini_uses_the_real_native_pcre2_provider() {
    let provider = provider();
    assert_eq!(unsafe { elephc_mbstring_mime_provider_v1(&provider) }, 0);
    let name = MbArgV1::string(b"mbstring.http_output_conv_mimetypes");
    let mut warnings = Vec::new();
    let initial = call(INI_GET, &[name], &mut warnings).3;
    assert_eq!(call(INI_SET, &[name, MbArgV1::string(b" \t[\0ignored\r\n")], &mut warnings), (0, RESULT_BOOL, 0, vec![]));
    assert_eq!(warnings, vec![(2, b"ini_set(): [ (offset=1): missing terminating ] for character class".to_vec())]);
    assert_eq!(call(INI_GET, &[name], &mut warnings).3, initial);
    warnings.clear();
    let pattern = b"(?<=^application/)json";
    assert_eq!(call(INI_SET, &[name, MbArgV1::string(pattern)], &mut warnings), (0, RESULT_STRING, 0, initial.clone()));
    assert_eq!(call(INI_GET, &[name], &mut warnings).3, pattern);
    let mut handle = std::ptr::null_mut();
    let mut offset = 0;
    assert_eq!(unsafe { provider.compile.unwrap()(&mut handle, pattern.as_ptr(), pattern.len() as u64, &mut offset) }, 0);
    assert_eq!(unsafe { provider.matches.unwrap()(handle, b"APPLICATION/JSON".as_ptr(), 16) }, 1);
    unsafe { provider.free.unwrap()(handle); }
    assert_eq!(call(INI_RESTORE, &[name], &mut warnings).1, RESULT_NULL);
    assert_eq!(call(INI_GET, &[name], &mut warnings).3, initial);
    assert_eq!(call(INI_SET, &[name, MbArgV1::string(b"\0 \t\r\n\x0b")], &mut warnings).1, RESULT_STRING);
    assert!(warnings.is_empty());
}
