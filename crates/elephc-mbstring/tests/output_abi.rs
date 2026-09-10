//! Purpose:
//! Verifies output conversion through the real request ABI and native PCRE2 MIME provider.
//!
//! Called from:
//! - Cargo's focused mbstring integration-test harness.
//!
//! Key details:
//! - PHP 8.5.10 codec fixtures remain independent of the response callback model.
//! - The host changes live settings during protected header publication to test borrow boundaries.
//! - Every owned bridge result is copied and released, including pending and fatal results.

use std::{ffi::c_void, io::{BufRead, BufReader}, sync::Once};
use elephc_builtin_contract::{RuntimeBuiltinId, mbstring_abi::{*, ini::*, output_handler::*}};
use elephc_mbstring::abi::*;
use flate2::read::GzDecoder;
use serde_json::Value;

#[path = "support/mime_provider.rs"]
mod mime_provider;

/// Installs one real PCRE2 provider and gives each test thread a clean request.
fn initialize() {
    static INSTALL: Once = Once::new();
    INSTALL.call_once(|| assert_eq!(unsafe { elephc_mbstring_mime_provider_v1(&mime_provider::provider()) }, 0));
    elephc_mbstring_reset_v1();
}

/// Tracks response MIME metadata, protected callback status, and actual proposed header bytes.
struct Response {
    mimetype: Option<Vec<u8>>, default_mimetype: Option<Vec<u8>>, send_default: bool, in_handler: bool,
    reads: usize, headers: Vec<Vec<u8>>, info_status: i32, header_status: i32,
    malformed: u8, reject: bool, change_settings: bool,
}

impl Default for Response {
    /// Creates the initial CLI response with no explicit MIME type and default content enabled.
    fn default() -> Self {
        Self { mimetype: None, default_mimetype: None, send_default: true, in_handler: false,
            reads: 0, headers: Vec::new(), info_status: 0, header_status: 0,
            malformed: 0, reject: false, change_settings: false }
    }
}

impl Response {
    /// Borrows this response independently of value-coercion callback versions and contexts.
    fn table(&mut self) -> MbOutputHostV1 {
        MbOutputHostV1 { version: 1, size: std::mem::size_of::<MbOutputHostV1>() as u32,
            context: (self as *mut Self).cast(), info: Some(info), header: Some(header) }
    }

    /// Runs one prepared output phase and consumes the bridge's owned result on every status.
    fn output(&mut self, input: &[u8], phase: i64) -> (i32, u64, Vec<u8>) {
        let mut result = MbResultV1::default();
        let status = unsafe { elephc_mbstring_output_v1(input.as_ptr(), input.len() as u64, phase, &self.table(), &mut result) };
        let (kind, _, bytes) = consume(result);
        (status, kind, bytes)
    }
}

/// Publishes borrowed response metadata without executing PHP or changing mbstring request state.
unsafe extern "C" fn info(context: *mut c_void, out: *mut MbOutputInfoV1) -> i32 {
    let response = unsafe { &mut *context.cast::<Response>() };
    response.reads += 1;
    let mut info = MbOutputInfoV1 {
        mimetype: response.mimetype.as_ref().map_or(std::ptr::null(), |bytes| bytes.as_ptr()),
        mimetype_len: response.mimetype.as_ref().map_or(0, |bytes| bytes.len() as u64),
        default_mimetype: response.default_mimetype.as_ref().map_or(std::ptr::null(), |bytes| bytes.as_ptr()),
        default_mimetype_len: response.default_mimetype.as_ref().map_or(0, |bytes| bytes.len() as u64),
        send_default_content_type: u64::from(response.send_default), in_handler: u64::from(response.in_handler),
    };
    match response.malformed {
        1 => info.mimetype_len = 1,
        2 => info.default_mimetype_len = 1,
        3 => info.send_default_content_type = 2,
        4 => info.in_handler = 2,
        5 => info.mimetype_len = u64::MAX,
        _ => {},
    }
    unsafe { out.write(info); }
    response.info_status
}

/// Copies the proposed header and optionally changes live request settings outside all Rust borrows.
unsafe extern "C" fn header(context: *mut c_void, bytes: *const u8, length: u64) -> i32 {
    let response = unsafe { &mut *context.cast::<Response>() };
    let bytes = unsafe { std::slice::from_raw_parts(bytes, length as usize) }.to_vec();
    if !response.reject {
        response.mimetype = Some(bytes[b"Content-Type: ".len()..].to_vec());
        response.send_default = false;
    }
    response.headers.push(bytes);
    if response.change_settings {
        let source = setting(RuntimeBuiltinId::MbInternalEncoding, &[MbArgV1::string(b"UTF-16LE")]);
        let destination = setting(RuntimeBuiltinId::MbHttpOutput, &[MbArgV1::string(b"UTF-8")]);
        if source.0 != RESULT_BOOL || source.1 != 1 || destination.0 != RESULT_BOOL || destination.1 != 1 { return 1; }
    }
    response.header_status
}

/// Copies an owned result's scalar metadata and bytes before releasing its allocator-owned payloads.
fn consume(mut result: MbResultV1) -> (u64, i64, Vec<u8>) {
    let bytes = if result.len == 0 { Vec::new() }
        else { unsafe { std::slice::from_raw_parts(result.bytes, result.len as usize).to_vec() } };
    let output = (result.kind, result.value, bytes);
    unsafe { elephc_mbstring_release_v1(&mut result); }
    output
}

/// Reads or changes the same real request settings used by native and eval mbstring calls.
fn setting(operation: RuntimeBuiltinId, args: &[MbArgV1]) -> (u64, i64, Vec<u8>) {
    let mut result = MbResultV1::default();
    unsafe { elephc_mbstring_call_v1(operation.as_u32(), args.as_ptr(), args.len() as u64, &mut result); }
    consume(result)
}

/// Requires a successful setter while keeping deprecation diagnostics owned by its result.
fn set(operation: RuntimeBuiltinId, arg: MbArgV1) {
    let result = setting(operation, &[arg]);
    assert_eq!((result.0, result.1), (RESULT_BOOL, 1), "{operation:?}");
}

/// Rejects unexpected diagnostic callbacks in these valid MIME directive mutations.
unsafe extern "C" fn diagnostic(_: *mut c_void, _: u32, _: *const u8, _: u64) -> i32 { 1 }

/// Applies an accepted raw MIME expression through its real INI validator and PCRE2 provider.
fn pattern(bytes: &[u8]) {
    let args = [MbArgV1::string(b"mbstring.http_output_conv_mimetypes"), MbArgV1::string(bytes)];
    let host = MbIniHostV1 { version: 1, size: std::mem::size_of::<MbIniHostV1>() as u32,
        context: std::ptr::null_mut(), diagnostic: Some(diagnostic) };
    let mut result = MbResultV1::default();
    assert_eq!(unsafe { elephc_mbstring_ini_v1(INI_SET, args.as_ptr(), 2, &host, &mut result) }, 0);
    assert_eq!(consume(result).0, RESULT_INI_STRING);
}

/// Restores one fixture's binary string from its independent hexadecimal representation.
fn bytes(value: &Value) -> Vec<u8> {
    let hex = value.as_str().unwrap();
    (0..hex.len()).step_by(2).map(|offset| u8::from_str_radix(&hex[offset..offset + 2], 16).unwrap()).collect()
}

/// Applies source, destination, and replacement changes without resetting decoder state.
fn configure(values: &Value) {
    for (field, operation) in [("from", RuntimeBuiltinId::MbInternalEncoding), ("to", RuntimeBuiltinId::MbHttpOutput)] {
        if let Some(name) = values[field].as_str() { set(operation, MbArgV1::string(name.as_bytes())); }
    }
    if let Some(code) = values["substitute"].as_i64() { set(RuntimeBuiltinId::MbSubstituteCharacter, MbArgV1::integer(code)); }
    else if let Some(mode) = values["substitute"].as_str() { set(RuntimeBuiltinId::MbSubstituteCharacter, MbArgV1::string(mode.as_bytes())); }
}

/// Replays all 1,818 PHP codec calls through the response callbacks and request-level C ABI.
#[test]
fn output_abi_codec_feeds_match_php() {
    initialize();
    let reader = BufReader::new(GzDecoder::new(include_bytes!("fixtures/output_handler.jsonl.gz").as_slice()));
    let mut requests = 0;
    let mut calls = 0;
    for line in reader.lines() {
        let case: Value = serde_json::from_str(&line.unwrap()).unwrap();
        elephc_mbstring_reset_v1();
        configure(&case);
        let mut response = Response::default();
        for (index, step) in case["steps"].as_array().unwrap().iter().enumerate() {
            configure(&case["changes"][index]);
            let before = setting(RuntimeBuiltinId::MbGetInfo, &[MbArgV1::string(b"illegal_chars")]).1 as u64;
            assert_eq!(response.output(&bytes(&step["input"]), step["phase"].as_i64().unwrap()),
                (0, RESULT_STRING, bytes(&step["output"])), "{case}, step {index}");
            let after = setting(RuntimeBuiltinId::MbGetInfo, &[MbArgV1::string(b"illegal_chars")]).1 as u64;
            assert_eq!(after.wrapping_sub(before), step["errors"].as_u64().unwrap(), "{case}, step {index}");
            calls += 1;
        }
        requests += 1;
    }
    assert_eq!((requests, calls), (264, 1818));
}

/// Uses real PCRE2 case folding, lookbehind, directive trimming, and C-string subject boundaries.
#[test]
fn output_abi_selects_actual_response_mime() {
    initialize();
    set(RuntimeBuiltinId::MbHttpOutput, MbArgV1::string(b"ISO-8859-1"));
    for (mime, converted) in [(b"application/json".as_slice(), false), (b"text/plain; charset=UTF-8", true),
        (b"APPLICATION/XHTML+XML", true), (b"image/png", false), (b"TEXT/plain\0ignored", true)] {
        let mut response = Response { mimetype: Some(mime.to_vec()), send_default: false, ..Response::default() };
        assert_eq!(response.output("é".as_bytes(), 9), (0, RESULT_STRING, if converted { b"\xe9".to_vec() } else { "é".as_bytes().to_vec() }));
        assert_eq!(response.headers.len(), usize::from(converted));
    }
    pattern(b" \t(?<=^application/)json\0ignored\r\n");
    let mut response = Response { mimetype: Some(b"APPLICATION/JSON; version=1".to_vec()), send_default: false, ..Response::default() };
    assert_eq!(response.output("é".as_bytes(), 9), (0, RESULT_STRING, b"\xe9".to_vec()));
    assert_eq!(response.headers, [b"Content-Type: APPLICATION/JSON; charset=ISO-8859-1".to_vec()]);
    pattern(b"\0 \t\r\n\x0b");
    assert_eq!(response.output("é".as_bytes(), 9), (0, RESULT_STRING, "é".as_bytes().to_vec()));
    response.send_default = true;
    response.default_mimetype = Some(Vec::new());
    assert_eq!(response.output("é".as_bytes(), 9), (0, RESULT_STRING, b"\xe9".to_vec()));
    assert_eq!(response.headers.last().unwrap(), b"Content-Type: ; charset=ISO-8859-1");
}

/// Suppresses response callbacks in pass/non-START phases and header publication inside a handler.
#[test]
fn output_abi_respects_phase_and_handler_boundaries() {
    initialize();
    set(RuntimeBuiltinId::MbHttpOutput, MbArgV1::string(b"pass"));
    let mut result = MbResultV1::default();
    assert_eq!(unsafe { elephc_mbstring_output_v1(b"abc".as_ptr(), 3, 9, std::ptr::null(), &mut result) }, 0);
    assert_eq!(consume(result), (RESULT_STRING, 0, b"abc".to_vec()));
    set(RuntimeBuiltinId::MbHttpOutput, MbArgV1::string(b"ISO-8859-1"));
    let mut response = Response { in_handler: true, ..Response::default() };
    assert_eq!(response.output("é".as_bytes(), 1), (0, RESULT_STRING, b"\xe9".to_vec()));
    assert_eq!(response.reads, 1);
    assert!(response.headers.is_empty());
    set(RuntimeBuiltinId::MbHttpOutput, MbArgV1::string(b"pass"));
    assert_eq!(response.output(b"", 8), (0, RESULT_STRING, vec![]));
    set(RuntimeBuiltinId::MbHttpOutput, MbArgV1::string(b"ISO-8859-1"));
    assert_eq!(response.output("é".as_bytes(), 0), (0, RESULT_STRING, b"\xe9".to_vec()));
    assert_eq!(response.output(b"", 8), (0, RESULT_STRING, vec![]));
    assert_eq!(response.output("é".as_bytes(), 0), (0, RESULT_STRING, "é".as_bytes().to_vec()));
    assert_eq!(response.reads, 1);
}

/// Keeps the captured destination while header publication mutates live source settings and MIME bytes.
#[test]
fn output_abi_header_publication_observes_live_source() {
    initialize();
    set(RuntimeBuiltinId::MbHttpOutput, MbArgV1::string(b"ISO-8859-1"));
    let mut response = Response { change_settings: true, ..Response::default() };
    assert_eq!(response.output(b"\xe9\0", 9), (0, RESULT_STRING, b"\xe9".to_vec()));
    assert_eq!(response.headers, [b"Content-Type: text/html; charset=ISO-8859-1".to_vec()]);
    assert_eq!(setting(RuntimeBuiltinId::MbHttpOutput, &[]).2, b"UTF-8");
    assert_eq!(setting(RuntimeBuiltinId::MbInternalEncoding, &[]).2, b"UTF-16LE");
}

/// Finishes conversion and END reset after a contained header throwable or ordinary header refusal.
#[test]
fn output_abi_header_failures_preserve_conversion_order() {
    initialize();
    set(RuntimeBuiltinId::MbHttpOutput, MbArgV1::string(b"ASCII"));
    let mut response = Response { header_status: 2, reject: true, ..Response::default() };
    let output = response.output("é".as_bytes(), 9);
    assert_eq!(output.0, 2);
    assert!(output.2.is_empty());
    assert_eq!(setting(RuntimeBuiltinId::MbGetInfo, &[MbArgV1::string(b"illegal_chars")]).1, 1);
    assert_eq!(response.output("é".as_bytes(), 0), (0, RESULT_STRING, "é".as_bytes().to_vec()));
    response.header_status = 0;
    assert_eq!(response.output("é".as_bytes(), 9), (0, RESULT_STRING, b"?".to_vec()));
    assert!(response.send_default);
}

/// Rejects invalid native metadata without conversion, while valid later calls remain usable.
#[test]
fn output_abi_rejects_invalid_metadata_without_state_changes() {
    initialize();
    set(RuntimeBuiltinId::MbHttpOutput, MbArgV1::string(b"ASCII"));
    for malformed in 1..=5 {
        let mut response = Response { malformed, ..Response::default() };
        assert_eq!(response.output("é".as_bytes(), 1).0, 1);
        assert!(response.headers.is_empty());
        assert_eq!(response.output("é".as_bytes(), 0), (0, RESULT_STRING, "é".as_bytes().to_vec()));
    }
    for info_status in [1, 2, -1, 3] {
        let mut response = Response { info_status, ..Response::default() };
        assert_eq!(response.output(b"abc", 1).0, 1);
        assert!(response.headers.is_empty());
    }
    assert_eq!(setting(RuntimeBuiltinId::MbGetInfo, &[MbArgV1::string(b"illegal_chars")]).1, 0);
    assert_eq!(Response::default().output("é".as_bytes(), 9), (0, RESULT_STRING, b"?".to_vec()));
}

/// Rejects incomplete host tables and invalid byte ranges before invoking response callbacks.
#[test]
fn output_abi_validates_host_and_input_contracts() {
    initialize();
    set(RuntimeBuiltinId::MbHttpOutput, MbArgV1::string(b"ASCII"));
    let mut response = Response::default();
    let valid = response.table();
    for host in [MbOutputHostV1 { version: 2, ..valid }, MbOutputHostV1 { size: 0, ..valid },
        MbOutputHostV1 { info: None, ..valid }, MbOutputHostV1 { header: None, ..valid }] {
        let mut result = MbResultV1::default();
        assert_eq!(unsafe { elephc_mbstring_output_v1(b"x".as_ptr(), 1, 1, &host, &mut result) }, 1);
        assert_eq!(consume(result).0, RESULT_FATAL);
    }
    for (input, length, host) in [(std::ptr::null(), 1, &valid as *const _),
        (b"x".as_ptr(), u64::MAX, &valid), (b"x".as_ptr(), 1, std::ptr::null())] {
        let mut result = MbResultV1::default();
        assert_eq!(unsafe { elephc_mbstring_output_v1(input, length, 1, host, &mut result) }, 1);
        assert_eq!(consume(result).0, RESULT_FATAL);
    }
    assert_eq!(response.reads, 0);
    assert!(response.headers.is_empty());
    let mut result = MbResultV1::default();
    assert_eq!(unsafe { elephc_mbstring_output_v1(std::ptr::null(), 0, 0, std::ptr::null(), &mut result) }, 0);
    assert_eq!(consume(result), (RESULT_STRING, 0, Vec::new()));
    assert_eq!(unsafe { elephc_mbstring_output_v1(std::ptr::null(), 0, 0, std::ptr::null(), std::ptr::null_mut()) }, 1);
    for header_status in [1, -1, 3] {
        let mut response = Response { header_status, ..Response::default() };
        assert_eq!(response.output("é".as_bytes(), 1).0, 1);
        assert_eq!(response.output("é".as_bytes(), 0), (0, RESULT_STRING, "é".as_bytes().to_vec()));
    }
    assert_eq!(setting(RuntimeBuiltinId::MbGetInfo, &[MbArgV1::string(b"illegal_chars")]).1, 0);
}
