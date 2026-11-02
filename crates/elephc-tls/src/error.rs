//! Purpose:
//! Preserves TLS connector and handshake error text for stream diagnostics.
//!
//! Called from:
//! - TLS connect, read and write failure paths.
//! - Generated runtime function-pointer queries used by fileinfo.
//!
//! Key details:
//! - Each thread owns its returned C string until its next TLS diagnostic mutation.

use std::cell::RefCell;
use std::ffi::{c_char, CString};

thread_local! {
    static LAST_ERROR: RefCell<Option<CString>> = const { RefCell::new(None) };
}

/// Records the operation's cause without letting embedded NULs escape into the C ABI.
pub(crate) fn record(error: impl std::fmt::Display) {
    let message = CString::new(error.to_string().replace('\0', "?")).unwrap_or_default();
    LAST_ERROR.with(|slot| *slot.borrow_mut() = Some(message));
}

/// Records a connector failure and returns the existing invalid-handle sentinel.
pub(crate) fn failure(error: impl std::fmt::Display) -> i64 { record(error); -1 }

/// Clears stale detail before the next stream operation, including rejected configurations.
#[no_mangle]
pub extern "C" fn elephc_tls_clear_error() {
    LAST_ERROR.with(|slot| *slot.borrow_mut() = None);
}

/// Borrows the calling thread's latest connector or handshake cause until its next TLS call.
#[no_mangle]
pub extern "C" fn elephc_tls_last_error() -> *const c_char {
    LAST_ERROR.with(|slot| slot.borrow().as_ref().map_or(c"".as_ptr(), |message| message.as_ptr()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A refused connector retains its cause until reset, independently of another thread's errors.
    #[test]
    fn tls_connector_failure_detail_is_thread_local() {
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let host = b"127.0.0.1";
        let handle = unsafe { super::super::elephc_tls_connect(host.as_ptr(), host.len(), port) };
        assert_eq!(handle, -1);
        let pointer = elephc_tls_last_error();
        let before = unsafe { std::ffi::CStr::from_ptr(pointer).to_owned() };
        assert!(before.to_string_lossy().contains("Connection refused"), "{before:?}");
        std::thread::spawn(|| record("other thread")).join().unwrap();
        assert_eq!(unsafe { std::ffi::CStr::from_ptr(pointer) }, before.as_c_str());
        elephc_tls_clear_error();
        assert!(unsafe { std::ffi::CStr::from_ptr(elephc_tls_last_error()) }.is_empty());
    }
}
