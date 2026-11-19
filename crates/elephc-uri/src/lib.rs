//! Purpose:
//! Stable, panic-free C ABI for PHP 8.5's `ext/uri`: RFC 3986 and WHATWG URL values behind
//! opaque integer handles, with binary-safe hex string transfer and a thread-local error log.
//!
//! Called from:
//! - elephc-generated code through the `extern "elephc_uri"` block in `src/uri_prelude`.
//!
//! Key details:
//! - Handles live in a thread-local slot table: `handle == index + 1`, and freed slots are
//!   reused, so a long-running program does not grow the table per parsed URL.
//! - Every string crossing the ABI is hex-encoded, so PHP's binary strings survive NUL bytes
//!   in inputs and error contexts.
//! - The error log is per thread and is cleared at the start of every operation; a successful
//!   WHATWG parse may still leave warnings for the caller to read.

mod errors;
mod rfc3986;
mod whatwg;

use std::cell::RefCell;
use std::ffi::{CStr, CString};
use std::os::raw::c_char;

use errors::{UriError, ValidationError};
use rfc3986::RfcUri;
use whatwg::WhatWgUri;

/// One PHP-visible URI value.
enum UriValue {
    /// A `Uri\Rfc3986\Uri`.
    Rfc(RfcUri),
    /// A `Uri\WhatWg\Url`.
    WhatWg(WhatWgUri),
}

thread_local! {
    /// Opaque handle table; `None` marks a free slot.
    static HANDLES: RefCell<Vec<Option<UriValue>>> = const { RefCell::new(Vec::new()) };
    /// Free slot indexes available for reuse.
    static FREE_SLOTS: RefCell<Vec<usize>> = const { RefCell::new(Vec::new()) };
    /// Most recent operation's message and validation log.
    static LAST: RefCell<Option<UriError>> = const { RefCell::new(None) };
    /// Backing store for the most recent hex string handed to PHP.
    static RETURN_TEXT: RefCell<CString> = RefCell::new(CString::default());
}

/// Clears the thread-local error log.
fn clear_last() {
    LAST.with(|last| *last.borrow_mut() = None);
}

/// Stores a failed operation's message and validation log.
fn set_error(error: UriError) {
    LAST.with(|last| *last.borrow_mut() = Some(error));
}

/// Stores a successful WHATWG parse's warnings without a message.
fn set_warnings(errors: Vec<ValidationError>) {
    LAST.with(|last| {
        *last.borrow_mut() = if errors.is_empty() {
            None
        } else {
            Some(UriError {
                message: String::new(),
                errors,
            })
        }
    });
}

/// Registers a value and returns its opaque handle.
fn insert(value: UriValue) -> u64 {
    HANDLES.with(|handles| {
        let mut handles = handles.borrow_mut();
        let reused = FREE_SLOTS.with(|free| free.borrow_mut().pop());
        match reused {
            Some(index) => {
                handles[index] = Some(value);
                (index + 1) as u64
            }
            None => {
                handles.push(Some(value));
                handles.len() as u64
            }
        }
    })
}

/// Removes and returns the value behind one handle.
fn take(handle: u64) -> Option<UriValue> {
    if handle == 0 {
        return None;
    }
    let index = (handle - 1) as usize;
    HANDLES.with(|handles| {
        let mut handles = handles.borrow_mut();
        let slot = handles.get_mut(index)?;
        let value = slot.take();
        if value.is_some() {
            FREE_SLOTS.with(|free| free.borrow_mut().push(index));
        }
        value
    })
}

/// Runs `action` against one live handle without removing it.
fn with_handle<R>(handle: u64, action: impl FnOnce(&UriValue) -> R) -> Option<R> {
    if handle == 0 {
        return None;
    }
    let index = (handle - 1) as usize;
    HANDLES.with(|handles| {
        let handles = handles.borrow();
        handles.get(index)?.as_ref().map(action)
    })
}

/// Runs `action` against one live handle with mutable access.
fn with_handle_mut<R>(handle: u64, action: impl FnOnce(&mut UriValue) -> R) -> Option<R> {
    if handle == 0 {
        return None;
    }
    let index = (handle - 1) as usize;
    HANDLES.with(|handles| {
        let mut handles = handles.borrow_mut();
        handles.get_mut(index)?.as_mut().map(action)
    })
}

/// Copies bytes into the thread-local return buffer and returns their hex text.
fn return_hex(bytes: &[u8]) -> *const c_char {
    let hex = hex_encode(bytes);
    RETURN_TEXT.with(|cell| {
        let owned = CString::new(hex).unwrap_or_default();
        *cell.borrow_mut() = owned;
        cell.borrow().as_ptr()
    })
}

/// Encodes bytes as lowercase hexadecimal ASCII.
fn hex_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(char::from(HEX_DIGITS[(byte >> 4) as usize]));
        out.push(char::from(HEX_DIGITS[(byte & 0x0f) as usize]));
    }
    out
}

/// Decodes hexadecimal ASCII into bytes, returning `None` for malformed input.
fn hex_decode(text: &str) -> Option<Vec<u8>> {
    let bytes = text.as_bytes();
    if bytes.len() % 2 != 0 {
        return None;
    }
    let mut out = Vec::with_capacity(bytes.len() / 2);
    let mut index = 0;
    while index < bytes.len() {
        let high = hex_digit(bytes[index])?;
        let low = hex_digit(bytes[index + 1])?;
        out.push((high << 4) | low);
        index += 2;
    }
    Some(out)
}

/// Returns one hexadecimal digit value.
fn hex_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// Reads a NUL-terminated hex string argument, treating a null pointer as empty.
///
/// # Safety
/// `pointer` must be null or point at a NUL-terminated byte sequence valid for the call.
unsafe fn read_hex(pointer: *const c_char) -> Option<Vec<u8>> {
    if pointer.is_null() {
        return Some(Vec::new());
    }
    let text = CStr::from_ptr(pointer).to_str().ok()?;
    hex_decode(text)
}

/// How lexbor treats an invalid UTF-8 sequence in the component being converted.
#[derive(Clone, Copy, PartialEq, Eq)]
enum WhatWgTextMode {
    /// Whole-URL parse: a trailing incomplete sequence is dropped, and a special-scheme path
    /// percent-encodes the ASCII byte that follows an invalid lead byte.
    Parse,
    /// A `withPath()` value: no trailing drop, and every invalid lead byte in the path also
    /// percent-encodes its following ASCII byte.
    Path,
    /// Query, fragment, or another component: only the invalid byte is percent-encoded.
    Other,
}

/// Converts PHP string bytes into the text lexbor parses.
///
/// Valid UTF-8 is preserved. An invalid start byte is percent-encoded (`\xFF` -> `%FF`); an
/// incomplete trailing sequence is dropped from a whole-URL parse (`?q=\xC3` loses the
/// sequence) but percent-encoded from a mutation (`withQuery("q=\xC3")` stores `q=%C3`).
/// Inside a special-scheme path, an invalid lead byte also percent-encodes the following
/// ASCII byte (`a\xC3b` -> `a%C3%62`), which is what php-src's lexbor path scanner does.
fn whatwg_text(bytes: &[u8], mode: WhatWgTextMode) -> String {
    let (special, path_start, path_end) = whatwg_path_bounds(bytes);
    let mut out = String::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        if byte < 0x80 {
            out.push(byte as char);
            index += 1;
            continue;
        }
        let width = match byte {
            0xC2..=0xDF => 2,
            0xE0..=0xEF => 3,
            0xF0..=0xF4 => 4,
            _ => 0,
        };
        if width > 0 {
            if index + width <= bytes.len() {
                if let Ok(text) = std::str::from_utf8(&bytes[index..index + width]) {
                    out.push_str(text);
                    index += width;
                    continue;
                }
            } else if mode == WhatWgTextMode::Parse {
                break;
            }
        }
        out.push_str(&format!("%{byte:02X}"));
        let in_special_path = mode == WhatWgTextMode::Path
            || (mode == WhatWgTextMode::Parse && special && index >= path_start && index < path_end);
        if width > 0
            && in_special_path
            && index + 1 < bytes.len()
            && bytes[index + 1] & 0xC0 != 0x80
        {
            out.push_str(&format!("%{:02X}", bytes[index + 1]));
            index += 2;
        } else {
            index += 1;
        }
    }
    out
}

/// Returns the special-scheme flag and the literal path byte range of a raw URL.
fn whatwg_path_bounds(bytes: &[u8]) -> (bool, usize, usize) {
    let Some(colon) = bytes.iter().position(|byte| *byte == b':') else {
        return (false, 0, bytes.len());
    };
    let scheme = String::from_utf8_lossy(&bytes[..colon]).to_ascii_lowercase();
    let special = matches!(
        scheme.as_str(),
        "ftp" | "file" | "http" | "https" | "ws" | "wss"
    );
    if !special {
        return (false, colon + 1, bytes.len());
    }
    let mut start = colon + 1;
    while start < bytes.len() && (bytes[start] == b'/' || bytes[start] == b'\\') {
        start += 1;
    }
    let end = bytes[start..]
        .iter()
        .position(|byte| *byte == b'?' || *byte == b'#')
        .map(|offset| start + offset)
        .unwrap_or(bytes.len());
    (true, start, end)
}

/// Returns php-src's malformed-component message for one RFC 3986 component id.
fn rfc_component_malformed_message(component: i64) -> String {
    let name = match component {
        0 => "scheme",
        3 => "userinfo",
        4 => "host",
        6 => "path",
        7 => "query",
        8 => "fragment",
        _ => return "Failed to update the URI".to_string(),
    };
    format!("The specified {name} is malformed")
}

/// Deep-clones one URI value.
fn clone_value(value: &UriValue) -> UriValue {
    match value {
        UriValue::Rfc(uri) => UriValue::Rfc(uri.clone()),
        UriValue::WhatWg(uri) => UriValue::WhatWg(uri.clone()),
    }
}

/// Parses a URI and returns a handle, or zero on failure.
#[no_mangle]
pub extern "C" fn elephc_uri_parse(kind: i64, input: *const c_char, base: u64) -> u64 {
    clear_last();
    let Some(bytes) = (unsafe { read_hex(input) }) else {
        return 0;
    };
    match kind {
        0 => match rfc3986::parse(&bytes) {
            Ok(reference) => {
                let resolved = if base == 0 {
                    Ok(reference)
                } else {
                    match with_handle(base, clone_value) {
                        Some(UriValue::Rfc(base)) => base.resolve(&reference),
                        _ => Err("The specified base URI must be absolute".to_string()),
                    }
                };
                match resolved {
                    Ok(uri) => insert(UriValue::Rfc(uri)),
                    Err(message) => {
                        set_error(UriError {
                            message,
                            errors: Vec::new(),
                        });
                        0
                    }
                }
            }
            Err(message) => {
                set_error(UriError {
                    message,
                    errors: Vec::new(),
                });
                0
            }
        },
        1 => {
            let text = whatwg_text(&bytes, WhatWgTextMode::Parse);
            let base_url = if base == 0 {
                None
            } else {
                match with_handle(base, |value| {
                    if let UriValue::WhatWg(uri) = value {
                        Some(uri.url.clone())
                    } else {
                        None
                    }
                }) {
                    Some(Some(url)) => Some(url),
                    _ => {
                        set_error(UriError {
                            message: "The specified URI is malformed".to_string(),
                            errors: Vec::new(),
                        });
                        return 0;
                    }
                }
            };
            match whatwg::parse(&text, base_url.as_ref()) {
                Ok((uri, warnings)) => {
                    set_warnings(warnings);
                    insert(UriValue::WhatWg(uri))
                }
                Err(error) => {
                    set_error(error);
                    0
                }
            }
        }
        _ => 0,
    }
}

/// Releases one URI handle.
#[no_mangle]
pub extern "C" fn elephc_uri_free(handle: u64) {
    drop(take(handle));
}

/// Deep-clones one URI handle, returning zero for an invalid handle.
#[no_mangle]
pub extern "C" fn elephc_uri_clone(handle: u64) -> u64 {
    clear_last();
    match with_handle(handle, clone_value) {
        Some(value) => insert(value),
        None => 0,
    }
}

/// Reads one string component as hex, or returns null for an absent component.
#[no_mangle]
pub extern "C" fn elephc_uri_component(
    handle: u64,
    component: i64,
    mode: i64,
) -> *const c_char {
    clear_last();
    let value = with_handle(handle, |value| match value {
        UriValue::Rfc(uri) => uri.component(component, mode),
        UriValue::WhatWg(uri) => uri.component(component, mode),
    });
    match value.flatten() {
        Some(text) => return_hex(text.as_bytes()),
        None => std::ptr::null(),
    }
}

/// Reads the RFC 3986 URI type ordinal, or `-1` for a non-RFC handle.
#[no_mangle]
pub extern "C" fn elephc_uri_uri_type(handle: u64) -> i64 {
    clear_last();
    with_handle(handle, |value| match value {
        UriValue::Rfc(uri) => uri.uri_type(),
        UriValue::WhatWg(_) => -1,
    })
    .unwrap_or(-1)
}

/// Reads the host type ordinal, or `-1` when the URI has no host.
#[no_mangle]
pub extern "C" fn elephc_uri_host_type(handle: u64) -> i64 {
    clear_last();
    with_handle(handle, |value| match value {
        UriValue::Rfc(uri) => uri.host_type(),
        UriValue::WhatWg(uri) => uri.host_type(),
    })
    .flatten()
    .unwrap_or(-1)
}

/// Validates one builder component without mutating any value.
///
/// Returns `0` when the value is valid, `1` for a malformed component, and `2` for a
/// WHATWG port that is outside the 16-bit range. `kind` selects the RFC 3986 (`0`) or
/// WHATWG (`1`) rules; `component` uses the same ordinals as `elephc_uri_component`.
#[no_mangle]
pub extern "C" fn elephc_uri_validate(kind: i64, component: i64, input: *const c_char) -> i64 {
    clear_last();
    let Some(bytes) = (unsafe { read_hex(input) }) else {
        return 1;
    };
    let Ok(value) = std::str::from_utf8(&bytes) else {
        return 1;
    };
    match kind {
        0 => {
            let valid = match component {
                rfc3986::COMPONENT_SCHEME => rfc3986::parse::is_valid_scheme(value),
                rfc3986::COMPONENT_USERINFO => rfc3986::parse::is_valid_userinfo(value),
                rfc3986::COMPONENT_HOST => rfc3986::parse_host_text(value).is_ok(),
                rfc3986::COMPONENT_PATH => {
                    rfc3986::parse::is_valid_path_for_write(value, false)
                }
                rfc3986::COMPONENT_QUERY | rfc3986::COMPONENT_FRAGMENT => {
                    rfc3986::parse::is_valid_query_or_fragment(value)
                }
                _ => false,
            };
            i64::from(!valid)
        }
        1 => {
            let valid = match component {
                rfc3986::COMPONENT_SCHEME => is_whatwg_scheme(value),
                _ => true,
            };
            i64::from(!valid)
        }
        _ => 1,
    }
}

/// Returns whether `value` matches the WHATWG scheme production.
fn is_whatwg_scheme(value: &str) -> bool {
    let mut bytes = value.bytes();
    match bytes.next() {
        Some(first) if first.is_ascii_alphabetic() => {}
        _ => return false,
    }
    bytes.all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'-' | b'.'))
}

/// Applies the WHATWG percent-encode set selected by `mode` and returns the ASCII result.
///
/// The input arrives hex-encoded; the result is hex-encoded too so the caller can rebuild the
/// exact byte string. `mode` matches `Uri\WhatWg\UrlPercentEncodingMode`'s declaration order.
#[no_mangle]
pub extern "C" fn elephc_uri_percent_encode(mode: i64, input: *const c_char) -> *const c_char {
    clear_last();
    let Some(bytes) = (unsafe { read_hex(input) }) else {
        return std::ptr::null();
    };
    let mut out = String::with_capacity(bytes.len());
    for byte in bytes {
        let encode = match mode {
            0 | 1 => userinfo_encode(byte),
            2 | 4 => c0_encode(byte),
            3 => path_encode(byte),
            5 => path_encode(byte) || byte == b'/',
            6 => query_encode(byte),
            7 => query_encode(byte) || byte == b'\'',
            8 => {
                if byte == b' ' {
                    out.push('+');
                    continue;
                }
                !(byte.is_ascii_alphanumeric() || matches!(byte, b'*' | b'-' | b'.' | b'_'))
            }
            9 => fragment_encode(byte),
            _ => return std::ptr::null(),
        };
        if encode {
            out.push('%');
            out.push_str(&format!("{byte:02X}"));
        } else {
            out.push(byte as char);
        }
    }
    return_hex(out.as_bytes())
}

/// C0 control percent-encode set.
fn c0_encode(byte: u8) -> bool {
    byte < 0x20 || byte > 0x7E
}

/// Fragment percent-encode set.
fn fragment_encode(byte: u8) -> bool {
    c0_encode(byte) || matches!(byte, b' ' | b'"' | b'<' | b'>' | b'`')
}

/// Query percent-encode set.
fn query_encode(byte: u8) -> bool {
    c0_encode(byte) || matches!(byte, b' ' | b'"' | b'#' | b'<' | b'>')
}

/// Path percent-encode set.
fn path_encode(byte: u8) -> bool {
    query_encode(byte) || matches!(byte, b'?' | b'^' | b'`' | b'{' | b'}')
}

/// Userinfo percent-encode set.
fn userinfo_encode(byte: u8) -> bool {
    path_encode(byte)
        || matches!(byte, b'/' | b':' | b';' | b'=' | b'@' | b'[' | b'\\' | b']' | b'|')
}

/// Reads the port, returning `-1` when the URI has no port.
#[no_mangle]
pub extern "C" fn elephc_uri_port(handle: u64) -> i64 {
    clear_last();
    let value = with_handle(handle, |value| match value {
        UriValue::Rfc(uri) => uri.port_value(),
        UriValue::WhatWg(uri) => uri.port_value(),
    });
    value.flatten().unwrap_or(-1)
}

/// Writes one string-or-null component and returns the new handle, or zero on failure.
#[no_mangle]
pub extern "C" fn elephc_uri_with(
    handle: u64,
    component: i64,
    value: *const c_char,
    is_null: i64,
) -> u64 {
    clear_last();
    let Some(bytes) = (unsafe { read_hex(value) }) else {
        return 0;
    };
    let value = with_handle(handle, |current| match current {
        UriValue::Rfc(uri) => {
            let text = match std::str::from_utf8(&bytes) {
                Ok(text) => text,
                Err(_) => {
                    return Err(UriError {
                        message: rfc_component_malformed_message(component),
                        errors: Vec::new(),
                    })
                }
            };
            rfc3986::with_component(uri, component, if is_null != 0 { None } else { Some(text) })
                .map(UriValue::Rfc)
                .map_err(|message| UriError {
                    message,
                    errors: Vec::new(),
                })
        }
        UriValue::WhatWg(uri) => {
            let mode = if component == 6 {
                WhatWgTextMode::Path
            } else {
                WhatWgTextMode::Other
            };
            let text = whatwg_text(&bytes, mode);
            uri.with_component(
                component,
                if is_null != 0 { None } else { Some(text.as_str()) },
            )
            .map(UriValue::WhatWg)
        }
    });
    match value {
        Some(Ok(value)) => insert(value),
        Some(Err(error)) => {
            set_error(error);
            0
        }
        None => {
            set_error(UriError {
                message: "Failed to access the URI".to_string(),
                errors: Vec::new(),
            });
            0
        }
    }
}

/// Returns the PHP-side memory charge units for one URI handle, or zero when invalid.
#[no_mangle]
pub extern "C" fn elephc_uri_charge_units(handle: u64) -> i64 {
    clear_last();
    with_handle(handle, |value| match value {
        UriValue::Rfc(uri) => uri.charge_units(),
        UriValue::WhatWg(uri) => uri.charge_units(),
    })
    .unwrap_or(0)
}

/// Installs the builder-normalized serialized opaque path on a WHATWG URL.
///
/// Input: `handle` = live WHATWG URL handle, `path` = hex-encoded serialized path.
/// Output: 1 when the handle names a WHATWG URL, 0 otherwise.
#[no_mangle]
pub extern "C" fn elephc_uri_set_opaque_path(handle: u64, path: *const c_char) -> i64 {
    clear_last();
    let Some(bytes) = (unsafe { read_hex(path) }) else {
        return 0;
    };
    let Ok(path) = String::from_utf8(bytes) else {
        return 0;
    };
    with_handle_mut(handle, |value| match value {
        UriValue::WhatWg(uri) => {
            uri.set_opaque_path(path);
            1
        }
        UriValue::Rfc(_) => 0,
    })
    .unwrap_or(0)
}

/// Writes the port and returns the new handle, or zero on failure.
#[no_mangle]
pub extern "C" fn elephc_uri_with_port(handle: u64, port: i64, is_null: i64) -> u64 {
    clear_last();
    let port = if is_null != 0 { None } else { Some(port) };
    let value = with_handle(handle, |current| match current {
        UriValue::Rfc(uri) => rfc3986::with_port(uri, port)
            .map(UriValue::Rfc)
            .map_err(|message| UriError {
                message,
                errors: Vec::new(),
            }),
        UriValue::WhatWg(uri) => uri.with_port(port).map(UriValue::WhatWg),
    });
    match value {
        Some(Ok(value)) => insert(value),
        Some(Err(error)) => {
            set_error(error);
            0
        }
        None => {
            set_error(UriError {
                message: "Failed to access the URI".to_string(),
                errors: Vec::new(),
            });
            0
        }
    }
}

/// Serializes one URI as hex, or returns null when the handle is invalid.
#[no_mangle]
pub extern "C" fn elephc_uri_to_string(
    handle: u64,
    mode: i64,
    exclude_fragment: i64,
) -> *const c_char {
    clear_last();
    let exclude = exclude_fragment != 0;
    let value = with_handle(handle, |value| match value {
        UriValue::Rfc(uri) => uri.to_string(mode != 0, exclude),
        UriValue::WhatWg(uri) => uri.to_string(mode >= 2, exclude),
    });
    match value {
        Some(text) => return_hex(text.as_bytes()),
        None => {
            set_error(UriError {
                message: "Failed to access the URI".to_string(),
                errors: Vec::new(),
            });
            std::ptr::null()
        }
    }
}

/// Compares two URIs with normalized serialization.
#[no_mangle]
pub extern "C" fn elephc_uri_equals(left: u64, right: u64, include_fragment: i64) -> i64 {
    clear_last();
    let include = include_fragment != 0;
    let result = with_handle(left, |left| {
        with_handle(right, |right| match (left, right) {
            (UriValue::Rfc(left), UriValue::Rfc(right)) => left.equals(right, include),
            (UriValue::WhatWg(left), UriValue::WhatWg(right)) => left.equals(right, include),
            _ => false,
        })
    });
    i64::from(result.flatten().unwrap_or(false))
}

/// Returns the last error message as hex, or null when there is no message.
#[no_mangle]
pub extern "C" fn elephc_uri_error_message() -> *const c_char {
    LAST.with(|last| match last.borrow().as_ref() {
        Some(error) if !error.message.is_empty() => return_hex(error.message.as_bytes()),
        _ => std::ptr::null(),
    })
}

/// Returns how many validation entries the last operation produced.
#[no_mangle]
pub extern "C" fn elephc_uri_error_count() -> i64 {
    LAST.with(|last| {
        last.borrow()
            .as_ref()
            .map_or(0, |error| error.errors.len() as i64)
    })
}

/// Returns one validation entry's context as hex.
#[no_mangle]
pub extern "C" fn elephc_uri_error_context(index: i64) -> *const c_char {
    let Some(context) = LAST.with(|last| {
        last.borrow()
            .as_ref()
            .and_then(|error| error.errors.get(index.max(0) as usize))
            .map(|error| error.context.clone())
    }) else {
        return std::ptr::null();
    };
    return_hex(&context)
}

/// Returns one validation entry's stable type code.
#[no_mangle]
pub extern "C" fn elephc_uri_error_type(index: i64) -> i64 {
    LAST.with(|last| {
        last.borrow()
            .as_ref()
            .and_then(|error| error.errors.get(index.max(0) as usize))
            .map_or(-1, |error| error.type_code)
    })
}

/// Returns whether one validation entry is fatal.
#[no_mangle]
pub extern "C" fn elephc_uri_error_failure(index: i64) -> i64 {
    LAST.with(|last| {
        last.borrow()
            .as_ref()
            .and_then(|error| error.errors.get(index.max(0) as usize))
            .map_or(0, |error| i64::from(error.failure))
    })
}

/// Clears the last error log.
#[no_mangle]
pub extern "C" fn elephc_uri_clear_errors() {
    clear_last();
}

/// Lowercase hexadecimal digits used by `hex_encode`.
const HEX_DIGITS: [u8; 16] = *b"0123456789abcdef";
