//! Purpose:
//! Exposes allocation-light progressive-search entry points for the shared mbregex session.
//!
//! Called from:
//! - Generated `mb_ereg_search*` calls whose arguments already have their PHP representations.
//!
//! Key details:
//! - The setter and getter operate on the same thread-local session as the generic bridge.
//! - `init` is restricted to the ASCII `[0-9]+` pattern by the codegen fast path; the step,
//!   position, and set-position entries are state-shape agnostic.
//! - Invalid subjects return zero, and a failed set-position returns zero so the caller can run
//!   the generic bridge and preserve PHP's exact ValueError diagnostics.

use std::panic::{catch_unwind, AssertUnwindSafe};
use super::*;
use crate::regex::Limits;

/// Initializes progressive search with the literal `[0-9]+` pattern.
///
/// # Safety
/// `subject` must be null or point to `len` readable bytes for the duration of the call.
#[no_mangle]
pub unsafe extern "C" fn elephc_mbstring_ereg_search_init_simple_v1(
    subject: *const u8,
    len: u64,
) -> i32 {
    catch_unwind(AssertUnwindSafe(|| unsafe {
        if !crate::regex::provider_available() || len > isize::MAX as u64 || (len != 0 && subject.is_null()) {
            return -1;
        }
        let bytes = if len == 0 {
            &[][..]
        } else {
            std::slice::from_raw_parts(subject, len as usize)
        };
        REGEX.with(|session| {
            match session.initialize_simple(bytes) {
                Ok(true) => 1,
                Ok(false) => 0,
                Err(_) => -1,
            }
        })
    }))
    .unwrap_or(-1)
}

/// Performs one progressive search using the session's retained pattern and subject.
#[no_mangle]
pub extern "C" fn elephc_mbstring_ereg_search_step_v1() -> i32 {
    catch_unwind(AssertUnwindSafe(|| {
        if !crate::regex::provider_available() {
            return -1;
        }
        REGEX.with(|session| {
            if !session.progressive_ready() {
                return -1;
            }
            let limits = Limits { stack: None, retry: None };
            match session.search(None, None, "mb_ereg_search", limits, &mut |_| {}) {
                Ok(Some(_)) => 1,
                Ok(None) => 0,
                Err(_) => -1,
            }
        })
    }))
    .unwrap_or(-1)
}

/// Returns the session's progressive byte position.
#[no_mangle]
pub extern "C" fn elephc_mbstring_ereg_search_getpos_v1() -> i64 {
    catch_unwind(AssertUnwindSafe(|| {
        REGEX.with(|session| i64::try_from(session.position()).unwrap_or(-1))
    }))
    .unwrap_or(-1)
}

/// Sets the progressive byte position, returning one on success and zero on range failure.
#[no_mangle]
pub extern "C" fn elephc_mbstring_ereg_search_setpos_v1(position: i64) -> i32 {
    catch_unwind(AssertUnwindSafe(|| {
        REGEX.with(|session| i32::from(session.set_position(position).is_ok()))
    }))
    .unwrap_or(0)
}

/// Runs one search step and reports the retained match's start and length.
///
/// Returns one on a match, zero when the search found nothing, and the negative sentinel when
/// the provider is not registered yet or the request has no progressive pattern/subject. The
/// caller then runs the generic bridge, which installs the provider and emits PHP diagnostics.
///
/// # Safety
/// Both output pointers must be null or address writable `i64` storage.
#[no_mangle]
pub unsafe extern "C" fn elephc_mbstring_ereg_search_pos_v1(
    start_out: *mut i64,
    len_out: *mut i64,
) -> i32 {
    if start_out.is_null() || len_out.is_null() || !crate::regex::provider_available() {
        return -1;
    }
    catch_unwind(AssertUnwindSafe(|| {
        REGEX.with(|session| {
            if !session.progressive_ready() {
                return -1;
            }
            if !session.simple_ready() {
                return -2;
            }
            let limits = Limits { stack: None, retry: None };
            match session.search(None, None, "mb_ereg_search_pos", limits, &mut |_| {}) {
                Ok(Some(_)) => match session.match_position() {
                    Some((start, len)) => {
                        unsafe {
                            *start_out = start as i64;
                            *len_out = len as i64;
                        }
                        1
                    }
                    None => 0,
                },
                Ok(None) => 0,
                Err(_) => -1,
            }
        })
    }))
    .unwrap_or(-1)
}

/// Runs one search step and borrows the complete group-0 match range.
///
/// The pointer stays valid only until the next session mutation; the caller copies it into the
/// returned PHP array immediately. Negative status selects the generic bridge.
///
/// # Safety
/// `ptr_out` and `len_out` must be null or address writable pointer/usize storage.
#[no_mangle]
pub unsafe extern "C" fn elephc_mbstring_ereg_search_regs_v1(
    ptr_out: *mut *const u8,
    len_out: *mut u64,
) -> i32 {
    if ptr_out.is_null() || len_out.is_null() || !crate::regex::provider_available() {
        return -1;
    }
    catch_unwind(AssertUnwindSafe(|| {
        REGEX.with(|session| {
            if !session.progressive_ready() {
                return -1;
            }
            if !session.simple_ready() {
                return -2;
            }
            let limits = Limits { stack: None, retry: None };
            match session.search(None, None, "mb_ereg_search_regs", limits, &mut |_| {}) {
                Ok(Some(_)) => match session.match_group0() {
                    Some((ptr, len)) => {
                        unsafe {
                            *ptr_out = ptr;
                            *len_out = len as u64;
                        }
                        1
                    }
                    None => 0,
                },
                Ok(None) => 0,
                Err(_) => -1,
            }
        })
    }))
    .unwrap_or(-1)
}

/// Borrows the previously retained group-0 match range without advancing the search.
///
/// # Safety
/// `ptr_out` and `len_out` must be null or address writable pointer/usize storage.
#[no_mangle]
pub unsafe extern "C" fn elephc_mbstring_ereg_search_getregs_v1(
    ptr_out: *mut *const u8,
    len_out: *mut u64,
) -> i32 {
    if ptr_out.is_null() || len_out.is_null() || !crate::regex::provider_available() {
        return -1;
    }
    catch_unwind(AssertUnwindSafe(|| {
        REGEX.with(|session| {
            if !session.simple_ready() {
                return -2;
            }
            match session.match_group0() {
                Some((ptr, len)) => {
                    unsafe {
                        *ptr_out = ptr;
                        *len_out = len as u64;
                    }
                    1
                }
                None => 0,
            }
        })
    }))
    .unwrap_or(-1)
}
