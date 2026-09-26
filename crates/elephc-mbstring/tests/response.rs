//! Purpose:
//! Verifies response MIME history and terminal commitment through the shared request ABI.
//!
//! Called from:
//! - The focused mbstring integration harness.
//!
//! Key details:
//! - Headers use PHP's first accepted MIME identity independently of later wire replacements.
//! - Empty and buffered output leave headers mutable until the actual terminal sink commits.
//! - Protected diagnostics and converter callbacks share state without overlapping Rust borrows.

use std::{ffi::c_void, sync::Once};
use elephc_builtin_contract::{RuntimeBuiltinId, mbstring_abi::{*, ini::*, output_handler::*}};
use elephc_mbstring::{abi::*, state::Response};

#[path = "support/mime_provider.rs"]
mod mime_provider;

/// Retains the live handler flag and diagnostic/header observations outside the shared request.
#[derive(Default)]
#[repr(C)]
struct Host { in_handler: u64, warnings: Vec<Vec<u8>>, headers: Vec<Vec<u8>> }

/// Copies protected diagnostics without mutating mbstring state or unwinding through its C ABI.
unsafe extern "C" fn diagnostic(context: *mut c_void, _: u32, bytes: *const u8, length: u64) -> i32 {
    let host = unsafe { &mut *context.cast::<Host>() };
    host.warnings.push(unsafe { std::slice::from_raw_parts(bytes, length as usize) }.to_vec());
    0
}

/// Copies and releases one bridge result, including normal rejection and fatal empty outputs.
fn consume(mut result: MbResultV1) -> (u64, Vec<u8>) {
    let bytes = if result.len == 0 { Vec::new() } else { unsafe { std::slice::from_raw_parts(result.bytes, result.len as usize) }.to_vec() };
    let kind = result.kind;
    unsafe { elephc_mbstring_release_v1(&mut result); }
    (kind, bytes)
}

/// Calls the real header state owner and retains accepted bytes for an independent host sink.
fn header(host: &mut Host, bytes: &[u8], origin: u32) -> (i32, u64) {
    let table = MbIniHostV1 { version: 1, size: std::mem::size_of::<MbIniHostV1>() as u32,
        context: (host as *mut Host).cast(), diagnostic: Some(diagnostic) };
    let mut result = MbResultV1::default();
    let status = unsafe { elephc_mbstring_response_header_v1(bytes.as_ptr(), bytes.len() as u64, origin, &table, &mut result) };
    let (kind, bytes) = consume(result);
    if status == 0 && kind == RESULT_STRING { host.headers.push(bytes); }
    (status, kind)
}

/// Publishes a converter-generated header through the same actual response state as header().
unsafe extern "C" fn output_header(context: *mut c_void, bytes: *const u8, length: u64) -> i32 {
    let bytes = unsafe { std::slice::from_raw_parts(bytes, length as usize) };
    header(unsafe { &mut *context.cast::<Host>() }, bytes, 1).0
}

/// Copies borrowed MIME/default bytes before another request mutation can invalidate their storage.
fn info(host: &mut Host) -> (Option<Vec<u8>>, Vec<u8>, bool, bool) {
    let mut result = MbOutputInfoV1::default();
    assert_eq!(unsafe { elephc_mbstring_response_info_v1((host as *mut Host).cast(), &mut result) }, 0);
    let mime = if result.mimetype.is_null() { None }
        else { Some(unsafe { std::slice::from_raw_parts(result.mimetype, result.mimetype_len as usize) }.to_vec()) };
    let default = unsafe { std::slice::from_raw_parts(result.default_mimetype, result.default_mimetype_len as usize) }.to_vec();
    (mime, default, result.send_default_content_type != 0, result.in_handler != 0)
}

/// Initializes the real MIME provider once and resets the current test thread's request.
fn initialize() {
    static PROVIDER: Once = Once::new();
    PROVIDER.call_once(|| assert_eq!(unsafe { elephc_mbstring_mime_provider_v1(&mime_provider::provider()) }, 0));
    elephc_mbstring_reset_v1();
}

/// Runs conversion with actual response metadata and actual protected header publication.
fn output(host: &mut Host, input: &[u8], phase: i64) -> Vec<u8> {
    let table = MbOutputHostV1 { version: 1, size: std::mem::size_of::<MbOutputHostV1>() as u32,
        context: (host as *mut Host).cast(), info: Some(elephc_mbstring_response_info_v1), header: Some(output_header) };
    let mut result = MbResultV1::default();
    assert_eq!(unsafe { elephc_mbstring_output_v1(input.as_ptr(), input.len() as u64, phase, &table, &mut result) }, 0);
    let (kind, bytes) = consume(result);
    assert_eq!(kind, RESULT_STRING);
    bytes
}

/// Selects a concrete destination while retaining the same request and response metadata.
fn latin1() {
    let argument = MbArgV1::string(b"ISO-8859-1");
    let mut result = MbResultV1::default();
    unsafe { elephc_mbstring_call_v1(RuntimeBuiltinId::MbHttpOutput.as_u32(), &argument, 1, &mut result); }
    assert_eq!((result.kind, result.value), (RESULT_BOOL, 1));
    consume(result);
}

/// Preserves PHP's first Content-Type after later replacements while keeping accepted wire bytes separate.
#[test]
fn response_first_mime_controls_output_conversion() {
    initialize();
    latin1();
    let mut host = Host::default();
    assert_eq!(header(&mut host, b"Content-Type: application/json", 0), (0, RESULT_STRING));
    assert_eq!(header(&mut host, b"Content-Type: text/plain", 0), (0, RESULT_STRING));
    assert_eq!(info(&mut host).0, Some(b"application/json".to_vec()));
    assert_eq!(host.headers.last().unwrap(), b"Content-type: text/plain;charset=UTF-8");
    assert_eq!(output(&mut host, "é".as_bytes(), 9), "é".as_bytes());
    elephc_mbstring_reset_v1();
    latin1();
    header(&mut host, b"Content-Type: text/plain", 0);
    header(&mut host, b"Content-Type: application/json", 0);
    assert_eq!(output(&mut host, "é".as_bytes(), 9), b"\xe9");
    assert_eq!(host.headers.last().unwrap(), b"Content-Type: text/plain; charset=ISO-8859-1");
    assert_eq!(info(&mut host).0, Some(b"text/plain;charset=UTF-8".to_vec()));
    assert!(host.warnings.is_empty());
}

/// Commits defaults only for nonempty terminal output and rejects late headers without replacing MIME state.
#[test]
fn response_terminal_commit_and_reset() {
    initialize();
    let mut host = Host::default();
    assert_eq!(info(&mut host), (None, b"text/html".to_vec(), true, false));
    assert_eq!(elephc_mbstring_response_commit_v1(0), 0);
    assert!(info(&mut host).2);
    assert_eq!(elephc_mbstring_response_commit_v1(1), 0);
    assert_eq!(info(&mut host), (Some(b"text/html; charset=UTF-8".to_vec()), b"text/html".to_vec(), false, false));
    assert_eq!(header(&mut host, b"Content-Type: image/png", 0), (0, RESULT_BOOL));
    assert_eq!(host.warnings, [b"header(): Cannot modify header information - headers already sent".to_vec()]);
    latin1();
    assert_eq!(output(&mut host, "é".as_bytes(), 9), b"\xe9");
    assert_eq!(host.warnings.last().unwrap(), b"mb_output_handler(): Cannot modify header information - headers already sent");
    host.in_handler = 1;
    let warnings = host.warnings.len();
    assert_eq!(output(&mut host, "é".as_bytes(), 9), b"\xe9");
    assert_eq!(host.warnings.len(), warnings);
    assert!(info(&mut host).3);
    elephc_mbstring_reset_v1();
    assert_eq!(info(&mut host), (None, b"text/html".to_vec(), true, true));
}

/// Preserves exact header spelling rules, validation order, and present-empty first MIME metadata.
#[test]
fn response_header_validation_and_charset_spelling() {
    let mut response = Response::default();
    assert_eq!(response.header(b""), Ok(None));
    assert_eq!(response.header(b"Content-Type : text/plain"), Ok(Some(b"Content-Type : text/plain".to_vec())));
    assert_eq!(response.mimetype(), None);
    assert!(response.header(b"Content-Type: text/plain\0ignored").is_err());
    assert!(response.header(b"Content-Type: text/plain\nX: ignored").is_err());
    assert_eq!(response.mimetype(), None);
    assert_eq!(response.header(b"content-type:   TEXT/plain \r\n"), Ok(Some(b"content-type:   TEXT/plain".to_vec())));
    assert_eq!(response.mimetype(), Some(b"TEXT/plain".as_slice()));
    let mut response = Response::default();
    assert_eq!(response.header(b"Content-Type:"), Ok(Some(b"Content-Type:".to_vec())));
    assert_eq!(response.mimetype(), Some(b"".as_slice()));
    response.header(b"Content-Type: text/plain").unwrap();
    assert_eq!(response.mimetype(), Some(b"".as_slice()));
    response.commit(1);
    assert!(response.headers_sent());
    assert_eq!(response.header(b""), Err("Cannot modify header information - headers already sent"));
}

/// Keeps empty defaults, validates raw startup values, and distinguishes explicit from default charset rules.
#[test]
fn response_startup_defaults() {
    let pairs = |values: &[(&[u8], &[u8])]| values.iter().map(|(name, value)| (name.to_vec(), value.to_vec())).collect::<Vec<_>>();
    let mut empty = Response::with_overrides(&pairs(&[(b"default_mimetype", b""), (b"default_charset", b"ISO-8859-1")]));
    empty.commit(1);
    assert_eq!(empty.mimetype(), None);
    let mut response = Response::with_overrides(&pairs(&[(b"default_mimetype", b"TEXT/custom"), (b"default_charset", b"ASCII")]));
    response.commit(1);
    assert_eq!(response.mimetype(), Some(b"TEXT/custom; charset=ASCII".as_slice()));
    let mut rejected = Response::with_overrides(&pairs(&[(b"default_mimetype", b"application/json"),
        (b"default_mimetype", b"bad\rvalue"), (b"default_charset", b"bad\0value")]));
    rejected.commit(1);
    assert_eq!(rejected.mimetype(), Some(b"text/html; charset=UTF-8".as_slice()));
    let mut response = Response::with_overrides(&pairs(&[(b"default_charset", b"")]));
    assert_eq!(response.header(b"Content-Type: text/plain"), Ok(Some(b"Content-Type: text/plain".to_vec())));
}
