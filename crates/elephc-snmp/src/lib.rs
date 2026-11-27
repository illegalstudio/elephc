//! Purpose:
//! C ABI and process-wide handle table for elephc's SNMP v1/v2c bridge.
//!
//! Called from:
//! - The injected `snmp` PHP prelude through `extern "elephc_snmp"`.
//!
//! Key details:
//! - A session owns the version, host, community, timeout and retry policy; a
//!   request returns a response handle whose varbinds are read one field at a
//!   time so PHP strings stay binary-safe.
//! - Output strings are thread-local and remain valid until the next bridge
//!   call on the same thread.

use std::cell::RefCell;
use std::collections::HashMap;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::ptr;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Mutex, OnceLock};

mod ber;
mod client;
mod v3;

use client::{Response, Value};

/// One open SNMP session.
#[derive(Clone, Debug)]
struct Session {
    /// SNMP version (0 = v1, 1 = v2c, 3 = v3).
    version: i64,
    /// Host and optional port.
    host: String,
    /// Community string bytes, or the v3 security name.
    community: Vec<u8>,
    /// Per-attempt timeout in milliseconds.
    timeout_ms: u64,
    /// Extra attempts after the first.
    retries: i64,
    /// Optional v3 auth protocol/password.
    auth: Option<(String, Vec<u8>)>,
    /// Optional v3 privacy protocol/password.
    privacy: Option<(String, Vec<u8>)>,
    /// Optional v3 context name.
    context_name: Vec<u8>,
    /// Optional v3 context engine ID.
    context_engine: Vec<u8>,
}

static SESSIONS: OnceLock<Mutex<HashMap<i64, Session>>> = OnceLock::new();
static RESPONSES: OnceLock<Mutex<HashMap<i64, Response>>> = OnceLock::new();
static NEXT_ID: AtomicI64 = AtomicI64::new(1);

thread_local! {
    /// Last string returned to PHP.
    static OUTPUT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

/// Returns the session table.
fn sessions() -> &'static Mutex<HashMap<i64, Session>> {
    SESSIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Returns the response table.
fn responses() -> &'static Mutex<HashMap<i64, Response>> {
    RESPONSES.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Stores `bytes` in the thread-local output buffer and returns its pointer.
fn publish(bytes: &[u8]) -> *const u8 {
    OUTPUT.with(|slot| {
        let mut buffer = slot.borrow_mut();
        buffer.clear();
        buffer.extend_from_slice(bytes);
        buffer.push(0);
        buffer.as_ptr()
    })
}

/// Converts a raw pointer/length pair into bytes.
///
/// # Safety
/// `data` must reference `len` readable bytes when `len` is nonzero.
unsafe fn bytes<'a>(data: *const u8, len: usize) -> Option<&'a [u8]> {
    if len == 0 {
        Some(&[])
    } else if data.is_null() {
        None
    } else {
        Some(std::slice::from_raw_parts(data, len))
    }
}

/// Runs `f`, converting panics into `fallback`.
fn guarded<T: Copy>(fallback: T, f: impl FnOnce() -> T) -> T {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or(fallback)
}

/// C ABI: opens an SNMP session, returning a handle or -1.
///
/// # Safety
/// `host` and `community` must reference their declared readable lengths.
#[no_mangle]
pub unsafe extern "C" fn elephc_snmp_open(
    version: i64,
    host: *const u8,
    host_len: usize,
    community: *const u8,
    community_len: usize,
    timeout_ms: i64,
    retries: i64,
) -> i64 {
    guarded(-1, || {
        let (Some(host), Some(community)) = (bytes(host, host_len), bytes(community, community_len))
        else {
            return -1;
        };
        let Ok(host) = std::str::from_utf8(host) else {
            return -1;
        };
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        sessions()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(
                id,
                Session {
                    version,
                    host: host.to_string(),
                    community: community.to_vec(),
                    timeout_ms: u64::try_from(timeout_ms.max(1)).unwrap_or(1_000),
                    retries,
                    auth: None,
                    privacy: None,
                    context_name: Vec::new(),
                    context_engine: Vec::new(),
                },
            );
        id
    })
}

/// C ABI: closes an SNMP session.
#[no_mangle]
pub extern "C" fn elephc_snmp_close(session: i64) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        sessions()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(&session);
    }));
}

/// C ABI: sets the SNMPv3 auth protocol/password; returns 0 or -1.
///
/// # Safety
/// Both pointers must reference their declared readable lengths.
#[no_mangle]
pub unsafe extern "C" fn elephc_snmp_set_auth(
    session: i64,
    protocol: *const u8,
    protocol_len: usize,
    password: *const u8,
    password_len: usize,
) -> i64 {
    guarded(-1, || {
        let (Some(protocol), Some(password)) = (
            bytes(protocol, protocol_len),
            bytes(password, password_len),
        ) else {
            return -1;
        };
        let Ok(protocol) = std::str::from_utf8(protocol) else {
            return -1;
        };
        let mut sessions = sessions()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let Some(session) = sessions.get_mut(&session) else {
            return -1;
        };
        session.auth = Some((protocol.to_string(), password.to_vec()));
        0
    })
}

/// C ABI: sets the SNMPv3 privacy protocol/password; returns 0 or -1.
///
/// # Safety
/// Both pointers must reference their declared readable lengths.
#[no_mangle]
pub unsafe extern "C" fn elephc_snmp_set_priv(
    session: i64,
    protocol: *const u8,
    protocol_len: usize,
    password: *const u8,
    password_len: usize,
) -> i64 {
    guarded(-1, || {
        let (Some(protocol), Some(password)) = (
            bytes(protocol, protocol_len),
            bytes(password, password_len),
        ) else {
            return -1;
        };
        let Ok(protocol) = std::str::from_utf8(protocol) else {
            return -1;
        };
        let mut sessions = sessions()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let Some(session) = sessions.get_mut(&session) else {
            return -1;
        };
        session.privacy = Some((protocol.to_string(), password.to_vec()));
        0
    })
}

/// C ABI: sets the SNMPv3 context name/engine ID; returns 0 or -1.
///
/// # Safety
/// Both pointers must reference their declared readable lengths.
#[no_mangle]
pub unsafe extern "C" fn elephc_snmp_set_context(
    session: i64,
    name: *const u8,
    name_len: usize,
    engine: *const u8,
    engine_len: usize,
) -> i64 {
    guarded(-1, || {
        let (Some(name), Some(engine)) = (bytes(name, name_len), bytes(engine, engine_len)) else {
            return -1;
        };
        let mut sessions = sessions()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let Some(session) = sessions.get_mut(&session) else {
            return -1;
        };
        session.context_name = name.to_vec();
        session.context_engine = engine.to_vec();
        0
    })
}

/// C ABI: sends a GetRequest/GetNextRequest; returns a response handle or -1.
///
/// # Safety
/// `oid` must reference `len` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn elephc_snmp_request(
    session: i64,
    oid: *const u8,
    len: usize,
    pdu: i64,
) -> i64 {
    guarded(-1, || {
        let Some(oid) = bytes(oid, len) else {
            return -1;
        };
        let Ok(oid) = std::str::from_utf8(oid) else {
            return -1;
        };
        let session = {
            let sessions = sessions()
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            sessions.get(&session).cloned()
        };
        let Some(session) = session else {
            return -1;
        };
        let response = if session.version == 3 {
            v3::request(&session, oid, pdu)
        } else {
            client::request(
                session.version,
                &session.host,
                &session.community,
                oid,
                pdu,
                session.timeout_ms,
                session.retries,
            )
        };
        let Ok(response) = response else {
            return -1;
        };
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        responses()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(id, response);
        id
    })
}

/// C ABI: returns the number of response varbinds, or -1.
#[no_mangle]
pub extern "C" fn elephc_snmp_response_count(response: i64) -> i64 {
    guarded(-1, || {
        responses()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get(&response)
            .map(|response| response.varbinds.len() as i64)
            .unwrap_or(-1)
    })
}

/// C ABI: returns the response error-status, or -1.
#[no_mangle]
pub extern "C" fn elephc_snmp_response_error_status(response: i64) -> i64 {
    guarded(-1, || {
        responses()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get(&response)
            .map(|response| response.error_status)
            .unwrap_or(-1)
    })
}

/// C ABI: returns the response error-index, or -1.
#[no_mangle]
pub extern "C" fn elephc_snmp_response_error_index(response: i64) -> i64 {
    guarded(-1, || {
        responses()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get(&response)
            .map(|response| response.error_index)
            .unwrap_or(-1)
    })
}

/// C ABI: returns one varbind OID, or null.
#[no_mangle]
pub extern "C" fn elephc_snmp_response_oid(response: i64, index: i64) -> *const u8 {
    guarded(ptr::null(), || {
        let Ok(index) = usize::try_from(index) else {
            return ptr::null();
        };
        responses()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get(&response)
            .and_then(|response| response.varbinds.get(index))
            .map(|varbind| publish(varbind.oid.as_bytes()))
            .unwrap_or(ptr::null())
    })
}

/// C ABI: returns one varbind's SNMP value type code, or -1.
#[no_mangle]
pub extern "C" fn elephc_snmp_response_type(response: i64, index: i64) -> i64 {
    guarded(-1, || {
        let Ok(index) = usize::try_from(index) else {
            return -1;
        };
        responses()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get(&response)
            .and_then(|response| response.varbinds.get(index))
            .map(|varbind| match varbind.value {
                Value::Int(_) => 0x02,
                Value::Bytes(_) => 0x04,
                Value::Oid(_) => 0x06,
                Value::Null => 0x05,
                Value::Ip(_) => 0x40,
                Value::NoSuchObject => 0x80,
                Value::NoSuchInstance => 0x81,
                Value::EndOfMibView => 0x82,
            })
            .unwrap_or(-1)
    })
}

/// C ABI: returns one varbind's value text (decimal, hex, OID or dotted IP), or null.
#[no_mangle]
pub extern "C" fn elephc_snmp_response_value(response: i64, index: i64) -> *const u8 {
    guarded(ptr::null(), || {
        let Ok(index) = usize::try_from(index) else {
            return ptr::null();
        };
        responses()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get(&response)
            .and_then(|response| response.varbinds.get(index))
            .map(|varbind| match &varbind.value {
                Value::Int(value) => publish(value.to_string().as_bytes()),
                Value::Bytes(value) => publish(hex(value).as_bytes()),
                Value::Oid(value) => publish(value.as_bytes()),
                Value::Null => publish(&[]),
                Value::Ip(value) => publish(value.as_bytes()),
                Value::NoSuchObject | Value::NoSuchInstance | Value::EndOfMibView => publish(&[]),
            })
            .unwrap_or(ptr::null())
    })
}

/// C ABI: returns the byte length of the last published string, excluding NUL.
#[no_mangle]
pub extern "C" fn elephc_snmp_string_len() -> usize {
    OUTPUT.with(|slot| slot.borrow().len().saturating_sub(1))
}

/// C ABI: frees a response handle.
#[no_mangle]
pub extern "C" fn elephc_snmp_response_free(response: i64) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        responses()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(&response);
    }));
}

/// Returns the lowercase hex encoding of `bytes`.
fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &byte in bytes {
        out.push(char::from(DIGITS[usize::from(byte >> 4)]));
        out.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Opening/closing a session and a bad request fail closed.
    #[test]
    fn session_and_request_fail_closed() {
        let host = b"127.0.0.1:1";
        let community = b"public";
        let session = unsafe {
            elephc_snmp_open(1, host.as_ptr(), host.len(), community.as_ptr(), community.len(), 10, 0)
        };
        assert!(session > 0);
        let oid = b".1.3.6.1.2.1.1.1.0";
        let response = unsafe { elephc_snmp_request(session, oid.as_ptr(), oid.len(), client::GET_REQUEST) };
        assert_eq!(response, -1);
        elephc_snmp_close(session);
    }
}
