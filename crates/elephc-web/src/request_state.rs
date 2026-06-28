//! Purpose:
//! Owns the per-worker request/response state the `--web` bridge shares with the
//! compiled PHP runtime: the output-capture flag the runtime reads, the
//! response-body buffer the runtime appends to, and the parsed incoming request
//! statics (method, URI, path, query string, headers, body) the web prelude
//! reads via C-ABI getters. Provides `elephc_web_write`, `set_request`, all
//! request getters, and buffer lifecycle helpers.
//!
//! Called from:
//! - The compiled `--web` runtime's `__rt_stdout_write` capture branch, which
//!   calls `elephc_web_write(ptr, len)` when `elephc_web_capture` is non-zero.
//! - `crate::server::elephc_web_run`, which sets capture, clears the buffer,
//!   runs the handler, and flushes the captured body.
//! - `crate::worker`, which calls `set_request` after parsing the HTTP request
//!   and before invoking the PHP handler.
//!
//! Key details:
//! - One process per prefork worker, single-threaded: each request runs to
//!   completion on the worker's one thread, so all process-statics are race-free.
//! - All access to `static mut` items goes through raw pointers
//!   (`core::ptr::addr_of_mut!` / `core::ptr::addr_of!`), never `&mut`/`&`
//!   references, to stay clear of the `static_mut_refs` lint (a hard error under
//!   the workspace's zero-warnings gate).

use std::ffi::{c_char, CString};

extern "C" {
    /// Per-request output-capture flag defined in the compiled program's runtime
    /// `.comm` storage (`elephc_web_capture`). Non-zero routes the runtime's
    /// `__rt_stdout_write` through `elephc_web_write` instead of the plain
    /// `write(1, …)` syscall. The compiler mangles this name per target, so the
    /// clean C name here resolves to `_elephc_web_capture` on macOS and
    /// `elephc_web_capture` on Linux — matching the runtime's `.comm` and load.
    static mut elephc_web_capture: u8;
}

/// Process-static per-worker response body. Bytes echoed by the PHP handler land
/// here while capture is enabled; the server scaffold flushes it to the client
/// (currently stdout) once the handler returns.
static mut RESPONSE_BODY: Vec<u8> = Vec::new();

/// Enables or disables per-request output capture by writing the runtime's
/// extern capture flag. When `on` is true, `__rt_stdout_write` routes echo
/// output to `elephc_web_write` (the buffer below) instead of stdout.
///
/// # Safety
/// Single-threaded per worker (see module docs): the extern flag is reached only
/// through a raw pointer, never a reference to the `static mut`.
pub fn set_capture(on: bool) {
    unsafe {
        core::ptr::write(core::ptr::addr_of_mut!(elephc_web_capture), u8::from(on));
    }
}

/// Clears the response-body buffer before a request begins, so each request
/// starts with an empty body regardless of the previous request's output.
pub fn clear_body() {
    // SAFETY: single-threaded per worker; the buffer is mutated through a raw
    // pointer to avoid forming a reference to the `static mut`.
    unsafe {
        (*core::ptr::addr_of_mut!(RESPONSE_BODY)).clear();
    }
}

/// Appends `len` bytes starting at `ptr` to the per-worker response body. This
/// is the real destination for captured PHP output: the compiled runtime's
/// `__rt_stdout_write` capture branch calls this with the same C ABI as the
/// Phase-1 stub (byte pointer + length, no return value).
///
/// # Safety
/// `ptr` must point to `len` valid bytes for the duration of the call. Single-
/// threaded per worker (see module docs), so the buffer append cannot race.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_write(ptr: *const u8, len: usize) {
    if ptr.is_null() || len == 0 {
        return;
    }
    let bytes = core::slice::from_raw_parts(ptr, len);
    (*core::ptr::addr_of_mut!(RESPONSE_BODY)).extend_from_slice(bytes);
}

/// Takes ownership of the accumulated response body, leaving the buffer empty for
/// the next request. The server scaffold writes the returned bytes to the client.
pub fn take_body() -> Vec<u8> {
    // SAFETY: single-threaded per worker; the buffer is replaced through a raw
    // pointer to avoid forming a reference to the `static mut`.
    unsafe { core::mem::take(&mut *core::ptr::addr_of_mut!(RESPONSE_BODY)) }
}

/// HTTP response status for the current request. Reset to 200 each request.
static mut RESPONSE_STATUS: u16 = 200;
/// Response headers for the current request, as (name, value) pairs in send order.
static mut RESPONSE_HEADERS: Vec<(String, String)> = Vec::new();

/// Sets the response status. `code <= 0` reads the current status without
/// changing it (backs `http_response_code()` with no argument); a positive code
/// sets the status and returns the PREVIOUS one.
///
/// # Safety
/// Single-threaded per worker; the status is reached only through raw pointers.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_set_status(code: i64) -> i64 {
    let prev = *core::ptr::addr_of!(RESPONSE_STATUS) as i64;
    if code > 0 {
        core::ptr::write(core::ptr::addr_of_mut!(RESPONSE_STATUS), code as u16);
    }
    prev
}

/// Parses the status code from an `"HTTP/x.y NNN reason"` line (the 2nd token).
fn status_from_http_line(line: &str) -> Option<i64> {
    line.split_whitespace().nth(1).and_then(|t| t.parse::<i64>().ok())
}

/// Implements PHP `header($line, $replace, $response_code)` entirely in Rust:
/// `HTTP/` and `Status:` status lines, `Location:`→302, replace-vs-append, and
/// the 3rd-argument status override. All PHP `header()` semantics live here.
///
/// # Safety
/// `ptr` must point to `len` valid bytes for the call. Single-threaded per worker.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_header(
    ptr: *const u8,
    len: usize,
    replace: i64,
    response_code: i64,
) {
    if ptr.is_null() {
        return;
    }
    let line = String::from_utf8_lossy(core::slice::from_raw_parts(ptr, len)).into_owned();

    // 3rd arg: an explicit positive code forces the status.
    if response_code > 0 {
        elephc_web_set_status(response_code);
    }

    // "HTTP/x.y NNN ..." sets the status and adds no header.
    if line.trim_start().starts_with("HTTP/") {
        if let Some(code) = status_from_http_line(line.trim_start()) {
            elephc_web_set_status(code);
        }
        return;
    }

    // Split "Name: Value" on the first ':'. A line without ':' is ignored.
    let Some(idx) = line.find(':') else {
        return;
    };
    let name = line[..idx].trim().to_string();
    let value = line[idx + 1..].trim().to_string();
    if name.is_empty() {
        return;
    }

    // "Status: NNN ..." sets the status and adds no header.
    if name.eq_ignore_ascii_case("Status") {
        if let Some(code) = value
            .split_whitespace()
            .next()
            .and_then(|t| t.parse::<i64>().ok())
        {
            elephc_web_set_status(code);
        }
        return;
    }

    // "Location: ..." implies 302 unless an explicit code was given or the
    // current status is already 201 or a 3xx.
    if name.eq_ignore_ascii_case("Location") && response_code == 0 {
        let cur = *core::ptr::addr_of!(RESPONSE_STATUS);
        if !(cur == 201 || (300..=399).contains(&cur)) {
            elephc_web_set_status(302);
        }
    }

    let headers = &mut *core::ptr::addr_of_mut!(RESPONSE_HEADERS);
    if replace != 0 {
        headers.retain(|(n, _)| !n.eq_ignore_ascii_case(&name));
    }
    headers.push((name, value));
}

/// Resets the response status (200) and clears the response headers. Called by
/// the worker before each request's handler runs.
pub fn reset_response() {
    // SAFETY: single-threaded per worker; reached only through raw pointers.
    unsafe {
        core::ptr::write(core::ptr::addr_of_mut!(RESPONSE_STATUS), 200);
        (*core::ptr::addr_of_mut!(RESPONSE_HEADERS)).clear();
    }
}

/// Returns the current response status code.
pub fn take_status() -> u16 {
    // SAFETY: single-threaded per worker; read through a raw pointer.
    unsafe { *core::ptr::addr_of!(RESPONSE_STATUS) }
}

/// Drains and returns the response headers for the current request.
pub fn take_headers() -> Vec<(String, String)> {
    // SAFETY: single-threaded per worker; replaced through a raw pointer.
    unsafe { core::mem::take(&mut *core::ptr::addr_of_mut!(RESPONSE_HEADERS)) }
}

// Per-worker current-request state. One request runs to completion on the
// worker's single thread before the next begins, so plain process statics are
// race-free (same invariant as RESPONSE_BODY).
static mut REQ_METHOD: Option<CString> = None;
static mut REQ_URI: Option<CString> = None;
static mut REQ_PATH: Option<CString> = None;
static mut REQ_QUERY: Option<CString> = None;
static mut REQ_HEADERS: Vec<(CString, CString)> = Vec::new();
static mut REQ_BODY: Vec<u8> = Vec::new();
/// Connection/server metadata for the current request, backing the rest of the
/// `$_SERVER` keys (`REMOTE_ADDR`, `SERVER_PORT`, `SERVER_PROTOCOL`, …).
static mut REQ_REMOTE_ADDR: Option<CString> = None;
static mut REQ_REMOTE_PORT: i64 = 0;
static mut REQ_SERVER_ADDR: Option<CString> = None;
static mut REQ_SERVER_PORT: i64 = 0;
static mut REQ_PROTOCOL: Option<CString> = None;
/// Request start time in whole Unix seconds (backs `$_SERVER['REQUEST_TIME']`).
static mut REQ_TIME: i64 = 0;

/// Connection/server metadata passed to `set_request` alongside the HTTP fields.
pub(crate) struct RequestMeta {
    pub remote_addr: String,
    pub remote_port: u16,
    pub server_addr: String,
    pub server_port: u16,
    /// HTTP protocol string, e.g. "HTTP/1.1".
    pub protocol: String,
}

/// Stores the parsed request for the current worker thread. Called by the
/// worker before invoking the PHP handler. Non-UTF8 / interior-NUL bytes in
/// header values are replaced (CString cannot hold a NUL), which is acceptable
/// for HTTP tokens; the raw body keeps every byte (it is exposed binary-safe).
pub(crate) fn set_request(
    method: String,
    uri: String,
    path: String,
    query: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
    meta: RequestMeta,
) {
    fn cstr(s: &str) -> CString {
        CString::new(s.replace('\0', "")).unwrap_or_default()
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    unsafe {
        core::ptr::write(core::ptr::addr_of_mut!(REQ_METHOD), Some(cstr(&method)));
        core::ptr::write(core::ptr::addr_of_mut!(REQ_URI), Some(cstr(&uri)));
        core::ptr::write(core::ptr::addr_of_mut!(REQ_PATH), Some(cstr(&path)));
        core::ptr::write(core::ptr::addr_of_mut!(REQ_QUERY), Some(cstr(&query)));
        let hs: Vec<(CString, CString)> =
            headers.iter().map(|(n, v)| (cstr(n), cstr(v))).collect();
        core::ptr::write(core::ptr::addr_of_mut!(REQ_HEADERS), hs);
        core::ptr::write(core::ptr::addr_of_mut!(REQ_BODY), body);
        core::ptr::write(core::ptr::addr_of_mut!(REQ_REMOTE_ADDR), Some(cstr(&meta.remote_addr)));
        core::ptr::write(core::ptr::addr_of_mut!(REQ_REMOTE_PORT), meta.remote_port as i64);
        core::ptr::write(core::ptr::addr_of_mut!(REQ_SERVER_ADDR), Some(cstr(&meta.server_addr)));
        core::ptr::write(core::ptr::addr_of_mut!(REQ_SERVER_PORT), meta.server_port as i64);
        core::ptr::write(core::ptr::addr_of_mut!(REQ_PROTOCOL), Some(cstr(&meta.protocol)));
        core::ptr::write(core::ptr::addr_of_mut!(REQ_TIME), now);
        // Invalidate the lazily-parsed multipart cache: it belongs to the prior request.
        core::ptr::write(core::ptr::addr_of_mut!(MULTIPART_CACHE), None);
    }
}

/// Returns the C-string pointer held in an Option<CString> static, or an empty
/// string pointer when unset. The pointer is valid until the static is next
/// written (i.e. until the next request) — the compiler copies it immediately.
unsafe fn opt_ptr(slot: *const Option<CString>) -> *const c_char {
    static EMPTY: [c_char; 1] = [0];
    match &*slot {
        Some(s) => s.as_ptr(),
        None => EMPTY.as_ptr(),
    }
}

/// Returns the HTTP request method (e.g. "GET"); empty string before the first request.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_method() -> *const c_char {
    opt_ptr(core::ptr::addr_of!(REQ_METHOD))
}

/// Returns the raw request URI target (e.g. "/a?b=1"); empty string before the first request.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_uri() -> *const c_char {
    opt_ptr(core::ptr::addr_of!(REQ_URI))
}

/// Returns the path component of the URI (e.g. "/a"); empty string before the first request.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_path() -> *const c_char {
    opt_ptr(core::ptr::addr_of!(REQ_PATH))
}

/// Returns the query string component of the URI (e.g. "b=1"), without the leading "?";
/// empty string when there is no query string or before the first request.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_query_string() -> *const c_char {
    opt_ptr(core::ptr::addr_of!(REQ_QUERY))
}

/// Returns the number of request headers received with the current request.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_header_count() -> i64 {
    (*core::ptr::addr_of!(REQ_HEADERS)).len() as i64
}

/// Returns the name of header at index `i` (zero-based), or an empty string when out of range.
/// The pointer is valid until the next request.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_header_name(i: i64) -> *const c_char {
    static EMPTY: [c_char; 1] = [0];
    let hs = &*core::ptr::addr_of!(REQ_HEADERS);
    match usize::try_from(i).ok().and_then(|i| hs.get(i)) {
        Some((n, _)) => n.as_ptr(),
        None => EMPTY.as_ptr(),
    }
}

/// Returns the value of header at index `i` (zero-based), or an empty string when out of range.
/// The pointer is valid until the next request.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_header_value(i: i64) -> *const c_char {
    static EMPTY: [c_char; 1] = [0];
    let hs = &*core::ptr::addr_of!(REQ_HEADERS);
    match usize::try_from(i).ok().and_then(|i| hs.get(i)) {
        Some((_, v)) => v.as_ptr(),
        None => EMPTY.as_ptr(),
    }
}

/// Returns a pointer to the raw request body bytes (binary-safe; not NUL-terminated).
/// The pointer is valid until the next request.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_body_ptr() -> *const u8 {
    (*core::ptr::addr_of!(REQ_BODY)).as_ptr()
}

/// Returns the raw request body length in bytes.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_body_len() -> i64 {
    (*core::ptr::addr_of!(REQ_BODY)).len() as i64
}

/// Returns the client IP address (e.g. "127.0.0.1"); backs `$_SERVER['REMOTE_ADDR']`.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_remote_addr() -> *const c_char {
    opt_ptr(core::ptr::addr_of!(REQ_REMOTE_ADDR))
}

/// Returns the client TCP port; backs `$_SERVER['REMOTE_PORT']`.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_remote_port() -> i64 {
    *core::ptr::addr_of!(REQ_REMOTE_PORT)
}

/// Returns the bound server IP address; backs `$_SERVER['SERVER_ADDR']`.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_server_addr() -> *const c_char {
    opt_ptr(core::ptr::addr_of!(REQ_SERVER_ADDR))
}

/// Returns the bound server port; backs `$_SERVER['SERVER_PORT']`.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_server_port() -> i64 {
    *core::ptr::addr_of!(REQ_SERVER_PORT)
}

/// Returns the HTTP protocol string (e.g. "HTTP/1.1"); backs `$_SERVER['SERVER_PROTOCOL']`.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_protocol() -> *const c_char {
    opt_ptr(core::ptr::addr_of!(REQ_PROTOCOL))
}

/// Returns the request start time in whole Unix seconds; backs `$_SERVER['REQUEST_TIME']`.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_request_time() -> i64 {
    *core::ptr::addr_of!(REQ_TIME)
}

/// Process environment snapshot, built once per worker (the env is fixed at fork)
/// and reused across requests. Backs `$_ENV`.
static mut ENV_CACHE: Option<Vec<(CString, CString)>> = None;

/// Returns the cached process-environment (name, value) pairs, building it on
/// first use. Single-threaded per worker, so the lazy init cannot race.
unsafe fn env_cache() -> &'static [(CString, CString)] {
    let slot = core::ptr::addr_of_mut!(ENV_CACHE);
    if (*slot).is_none() {
        let vars: Vec<(CString, CString)> = std::env::vars()
            .map(|(k, v)| {
                (
                    CString::new(k.replace('\0', "")).unwrap_or_default(),
                    CString::new(v.replace('\0', "")).unwrap_or_default(),
                )
            })
            .collect();
        core::ptr::write(slot, Some(vars));
    }
    (*slot).as_deref().unwrap_or(&[])
}

/// Returns the number of process environment variables; backs `$_ENV`.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_env_count() -> i64 {
    env_cache().len() as i64
}

/// Returns the name of environment variable at index `i`, or empty when out of range.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_env_name(i: i64) -> *const c_char {
    static EMPTY: [c_char; 1] = [0];
    match usize::try_from(i).ok().and_then(|i| env_cache().get(i)) {
        Some((n, _)) => n.as_ptr(),
        None => EMPTY.as_ptr(),
    }
}

/// Returns the value of environment variable at index `i`, or empty when out of range.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_env_value(i: i64) -> *const c_char {
    static EMPTY: [c_char; 1] = [0];
    match usize::try_from(i).ok().and_then(|i| env_cache().get(i)) {
        Some((_, v)) => v.as_ptr(),
        None => EMPTY.as_ptr(),
    }
}

/// Lazily-parsed `multipart/form-data` parts for the current request, as
/// (name, filename, content_type, content). Invalidated each request in `set_request`.
#[allow(clippy::type_complexity)]
static mut MULTIPART_CACHE: Option<Vec<(CString, CString, CString, Vec<u8>)>> = None;

/// Returns the parsed multipart parts, parsing the request body on first use.
/// Empty unless the request is `multipart/form-data`. Single-threaded per worker.
unsafe fn multipart_parts() -> &'static [(CString, CString, CString, Vec<u8>)] {
    let slot = core::ptr::addr_of_mut!(MULTIPART_CACHE);
    if (*slot).is_none() {
        let content_type = (*core::ptr::addr_of!(REQ_HEADERS))
            .iter()
            .find(|(n, _)| n.to_bytes().eq_ignore_ascii_case(b"content-type"))
            .map(|(_, v)| v.to_string_lossy().into_owned())
            .unwrap_or_default();
        let body = (*core::ptr::addr_of!(REQ_BODY)).clone();
        let cstr = |s: &str| CString::new(s.replace('\0', "")).unwrap_or_default();
        let cached: Vec<(CString, CString, CString, Vec<u8>)> =
            crate::multipart::parse(&body, &content_type)
                .into_iter()
                .map(|p| {
                    (
                        cstr(&p.name),
                        cstr(p.filename.as_deref().unwrap_or("")),
                        cstr(&p.content_type),
                        p.content,
                    )
                })
                .collect();
        core::ptr::write(slot, Some(cached));
    }
    (*slot).as_deref().unwrap_or(&[])
}

/// Returns the number of `multipart/form-data` parts in the current request.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_multipart_count() -> i64 {
    multipart_parts().len() as i64
}

/// Returns the `name` of multipart part `i`, or empty when out of range.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_multipart_name(i: i64) -> *const c_char {
    static EMPTY: [c_char; 1] = [0];
    match usize::try_from(i).ok().and_then(|i| multipart_parts().get(i)) {
        Some((n, _, _, _)) => n.as_ptr(),
        None => EMPTY.as_ptr(),
    }
}

/// Returns the `filename` of multipart part `i` (empty for non-file fields / out of range).
#[no_mangle]
pub unsafe extern "C" fn elephc_web_multipart_filename(i: i64) -> *const c_char {
    static EMPTY: [c_char; 1] = [0];
    match usize::try_from(i).ok().and_then(|i| multipart_parts().get(i)) {
        Some((_, f, _, _)) => f.as_ptr(),
        None => EMPTY.as_ptr(),
    }
}

/// Returns the `Content-Type` of multipart part `i`, or empty when out of range.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_multipart_type(i: i64) -> *const c_char {
    static EMPTY: [c_char; 1] = [0];
    match usize::try_from(i).ok().and_then(|i| multipart_parts().get(i)) {
        Some((_, _, t, _)) => t.as_ptr(),
        None => EMPTY.as_ptr(),
    }
}

/// Returns a pointer to the raw content bytes of multipart part `i` (binary-safe).
#[no_mangle]
pub unsafe extern "C" fn elephc_web_multipart_value_ptr(i: i64) -> *const u8 {
    static EMPTY: [u8; 1] = [0];
    match usize::try_from(i).ok().and_then(|i| multipart_parts().get(i)) {
        Some((_, _, _, c)) => c.as_ptr(),
        None => EMPTY.as_ptr(),
    }
}

/// Returns the content length in bytes of multipart part `i`, or 0 when out of range.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_multipart_value_len(i: i64) -> i64 {
    match usize::try_from(i).ok().and_then(|i| multipart_parts().get(i)) {
        Some((_, _, _, c)) => c.len() as i64,
        None => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies set_request round-trips through the C-ABI getters.
    #[test]
    fn request_getters_round_trip() {
        use std::ffi::CStr;
        set_request(
            "POST".into(),
            "/p?x=1".into(),
            "/p".into(),
            "x=1".into(),
            vec![("Content-Type".into(), "text/plain".into())],
            b"hello".to_vec(),
            RequestMeta {
                remote_addr: "10.0.0.7".into(),
                remote_port: 54321,
                server_addr: "127.0.0.1".into(),
                server_port: 8080,
                protocol: "HTTP/1.1".into(),
            },
        );
        unsafe {
            assert_eq!(CStr::from_ptr(elephc_web_method()).to_str().unwrap(), "POST");
            assert_eq!(CStr::from_ptr(elephc_web_uri()).to_str().unwrap(), "/p?x=1");
            assert_eq!(CStr::from_ptr(elephc_web_path()).to_str().unwrap(), "/p");
            assert_eq!(CStr::from_ptr(elephc_web_query_string()).to_str().unwrap(), "x=1");
            assert_eq!(elephc_web_header_count(), 1);
            assert_eq!(CStr::from_ptr(elephc_web_header_name(0)).to_str().unwrap(), "Content-Type");
            assert_eq!(CStr::from_ptr(elephc_web_header_value(0)).to_str().unwrap(), "text/plain");
            assert_eq!(elephc_web_body_len(), 5);
            let body = std::slice::from_raw_parts(elephc_web_body_ptr(), 5);
            assert_eq!(body, b"hello");
            assert_eq!(CStr::from_ptr(elephc_web_remote_addr()).to_str().unwrap(), "10.0.0.7");
            assert_eq!(elephc_web_remote_port(), 54321);
            assert_eq!(CStr::from_ptr(elephc_web_server_addr()).to_str().unwrap(), "127.0.0.1");
            assert_eq!(elephc_web_server_port(), 8080);
            assert_eq!(CStr::from_ptr(elephc_web_protocol()).to_str().unwrap(), "HTTP/1.1");
            assert!(elephc_web_request_time() > 0);
        }
    }

    /// Verifies the bridge response logic matches PHP header()/http_response_code().
    #[test]
    fn response_control_matches_php() {
        unsafe {
            reset_response();
            assert_eq!(take_status(), 200); // default

            // http_response_code: set returns previous; 0 reads.
            reset_response();
            assert_eq!(elephc_web_set_status(404), 200);
            assert_eq!(elephc_web_set_status(0), 404);

            // Regular header, default replace=true → same-name (case-insensitive) replaced.
            reset_response();
            let a = b"X-Foo: a";
            elephc_web_header(a.as_ptr(), a.len(), 1, 0);
            let b = b"x-foo: b";
            elephc_web_header(b.as_ptr(), b.len(), 1, 0);
            assert_eq!(take_headers(), vec![("x-foo".to_string(), "b".to_string())]);

            // replace=false → append duplicates.
            reset_response();
            let c = b"X: 1";
            elephc_web_header(c.as_ptr(), c.len(), 0, 0);
            let d = b"X: 2";
            elephc_web_header(d.as_ptr(), d.len(), 0, 0);
            assert_eq!(
                take_headers(),
                vec![("X".into(), "1".into()), ("X".into(), "2".into())]
            );

            // Value keeps later colons; Location implies 302.
            reset_response();
            let l = b"Location: http://h/p:8080/x";
            elephc_web_header(l.as_ptr(), l.len(), 1, 0);
            assert_eq!(take_status(), 302);
            assert_eq!(
                take_headers(),
                vec![("Location".into(), "http://h/p:8080/x".into())]
            );

            // HTTP status line sets status, adds no header.
            reset_response();
            let h = b"HTTP/1.1 404 Not Found";
            elephc_web_header(h.as_ptr(), h.len(), 1, 0);
            assert_eq!(take_status(), 404);
            assert!(take_headers().is_empty());

            // "Status:" sets status, adds no header.
            reset_response();
            let s = b"Status: 503 Unavailable";
            elephc_web_header(s.as_ptr(), s.len(), 1, 0);
            assert_eq!(take_status(), 503);
            assert!(take_headers().is_empty());

            // 3rd arg forces status (Location does not override an explicit code).
            reset_response();
            let r = b"Location: /perm";
            elephc_web_header(r.as_ptr(), r.len(), 1, 301);
            assert_eq!(take_status(), 301);

            // Location does NOT downgrade an already-3xx status to 302.
            reset_response();
            elephc_web_set_status(303);
            let r2 = b"Location: /x";
            elephc_web_header(r2.as_ptr(), r2.len(), 1, 0);
            assert_eq!(take_status(), 303);
        }
    }
}
