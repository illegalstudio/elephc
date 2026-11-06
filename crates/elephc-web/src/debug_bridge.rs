//! Purpose:
//! Calls the optional per-request debug route hook installed by a
//! `--debug --web` master boot, before and after the PHP handler.
//!
//! Called from:
//! - `crate::worker::serve_inner()` around every request dispatch, including
//!   application-lifecycle workers.
//!
//! Key details:
//! - The hook slots are generated `.comm` words, null in a regular `--web`
//!   binary, so a normal server pays one load and a branch per request.
//! - Requests are passed as method/path slices plus a NUL-separated flat header
//!   buffer; interior NUL bytes in header values become spaces because NUL is
//!   the field separator.
//! - The hook never runs in the master; only a forked worker calls it.

use std::mem;

extern "C" {
    /// Nullable per-request route hook installed by the debug web master boot.
    static elephc_debug_web_request_fn: usize;
    /// Nullable per-request session teardown installed by the same boot.
    static elephc_debug_web_request_end_fn: usize;
}

/// Signature of the runtime's route hook.
type RequestHook = extern "C" fn(
    method: *const u8,
    method_len: usize,
    path: *const u8,
    path_len: usize,
    headers: *const u8,
    headers_len: usize,
);

/// Signature of the runtime's session teardown.
type EndHook = extern "C" fn();

/// Offers one request to the debug route hook, if this binary carries one.
pub(crate) fn request_begin(method: &str, path: &str, headers: &[(String, String)]) {
    let hook = unsafe { core::ptr::addr_of!(elephc_debug_web_request_fn).read() };
    if hook == 0 {
        return;
    }
    let mut buffer = Vec::new();
    for (name, value) in headers {
        append_field(&mut buffer, name.as_bytes());
        append_field(&mut buffer, value.as_bytes());
    }
    let hook: RequestHook = unsafe { mem::transmute(hook) };
    hook(
        method.as_ptr(),
        method.len(),
        path.as_ptr(),
        path.len(),
        buffer.as_ptr(),
        buffer.len(),
    );
}

/// Ends the request's debug session, if the route hook armed one.
pub(crate) fn request_end() {
    let hook = unsafe { core::ptr::addr_of!(elephc_debug_web_request_end_fn).read() };
    if hook == 0 {
        return;
    }
    let hook: EndHook = unsafe { mem::transmute(hook) };
    hook();
}

/// Appends one NUL-terminated field, replacing interior NUL bytes with spaces.
fn append_field(buffer: &mut Vec<u8>, field: &[u8]) {
    buffer.extend(field.iter().map(|byte| if *byte == 0 { b' ' } else { *byte }));
    buffer.push(0);
}
