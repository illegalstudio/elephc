//! Purpose:
//! Exposes allocation-free mbstring catalog queries used by native count fast paths.
//!
//! Called from:
//! - Generated `count(mb_encoding_aliases(...))` and `count(mb_list_encodings())` helpers.
//!
//! Key details:
//! - Both functions return counts only; no array or string is materialized.
//! - An unknown encoding returns `-1` so the caller can run the shared bridge and keep PHP's
//!   ValueError diagnostics.
//! - The names are read as a borrowed byte slice; no callback or request-state borrow crosses
//!   the C boundary.

use std::panic::{catch_unwind, AssertUnwindSafe};

use crate::encoding::{Encoding, EncodingList};

/// Returns the number of public aliases for an encoding, or `-1` when it is unknown.
///
/// # Safety
/// `name` must be null or point to `len` readable bytes for the duration of the call.
#[no_mangle]
pub unsafe extern "C" fn elephc_mbstring_encoding_aliases_count_v1(
    name: *const u8,
    len: u64,
) -> i64 {
    catch_unwind(AssertUnwindSafe(|| {
        if len > isize::MAX as u64 || (len != 0 && name.is_null()) {
            return -1;
        }
        let bytes = if len == 0 {
            &[][..]
        } else {
            unsafe { std::slice::from_raw_parts(name, len as usize) }
        };
        Encoding::lookup_c_string(bytes).map_or(-1, |encoding| encoding.aliases().len() as i64)
    }))
    .unwrap_or(-1)
}

/// Returns the byte length of the current internal encoding name.
#[no_mangle]
pub extern "C" fn elephc_mbstring_internal_encoding_length_v1() -> i64 {
    catch_unwind(AssertUnwindSafe(|| {
        super::REQUEST.with(|state| state.borrow().internal_encoding().name().len() as i64)
    }))
    .unwrap_or(-1)
}

/// Applies one string setting and reports whether the value was valid.
///
/// # Safety
/// `name` must be null or point to `len` readable bytes for the duration of the call.
unsafe fn set_string_setting(
    name: *const u8,
    len: u64,
    apply: impl FnOnce(&[u8]) -> bool,
) -> i32 {
    if len > isize::MAX as u64 || (len != 0 && name.is_null()) {
        return 0;
    }
    let bytes = if len == 0 {
        &[][..]
    } else {
        unsafe { std::slice::from_raw_parts(name, len as usize) }
    };
    i32::from(apply(bytes))
}

/// Sets the request internal encoding, returning `1` on success and `0` on validation failure.
#[no_mangle]
pub unsafe extern "C" fn elephc_mbstring_internal_encoding_set_v1(
    name: *const u8,
    len: u64,
) -> i32 {
    // Re-applying the already-explicit UTF-8 default is a no-op; keep it out of the panic
    // boundary and the encoding parser because request-local code asserts it in hot loops.
    if len <= isize::MAX as u64 && (len == 0 || !name.is_null()) {
        let bytes = if len == 0 {
            &[][..]
        } else {
            unsafe { std::slice::from_raw_parts(name, len as usize) }
        };
        let unchanged = super::REQUEST.with(|state| {
            let state = state.borrow();
            state.internal_is_explicit()
                && state.internal_encoding().name().eq_ignore_ascii_case("UTF-8")
                && bytes.eq_ignore_ascii_case(b"UTF-8")
        });
        if unchanged {
            return 1;
        }
    }
    catch_unwind(AssertUnwindSafe(|| unsafe {
        set_string_setting(name, len, |bytes| {
            super::REQUEST.with(|state| state.borrow_mut().set_internal_encoding(bytes).is_ok())
        })
    }))
    .unwrap_or(0)
}

/// Sets the request language, returning `1` on success and `0` on validation failure.
#[no_mangle]
pub unsafe extern "C" fn elephc_mbstring_language_set_v1(name: *const u8, len: u64) -> i32 {
    catch_unwind(AssertUnwindSafe(|| unsafe {
        set_string_setting(name, len, |bytes| {
            super::REQUEST.with(|state| state.borrow_mut().set_language(bytes).is_ok())
        })
    }))
    .unwrap_or(0)
}

/// Sets the HTTP output encoding, returning `1` on success and `0` on validation failure.
#[no_mangle]
pub unsafe extern "C" fn elephc_mbstring_http_output_set_v1(name: *const u8, len: u64) -> i32 {
    catch_unwind(AssertUnwindSafe(|| unsafe {
        set_string_setting(name, len, |bytes| {
            super::REQUEST.with(|state| state.borrow_mut().set_http_output(bytes).is_ok())
        })
    }))
    .unwrap_or(0)
}

/// Sets the regex encoding, returning `1` on success and `0` on validation failure.
#[no_mangle]
pub unsafe extern "C" fn elephc_mbstring_regex_encoding_set_v1(name: *const u8, len: u64) -> i32 {
    catch_unwind(AssertUnwindSafe(|| unsafe {
        set_string_setting(name, len, |bytes| {
            super::REGEX.with(|session| session.set_encoding(bytes).is_ok())
        })
    }))
    .unwrap_or(0)
}

/// Sets the substitute codepoint, returning `1` on success and `0` on validation failure.
#[no_mangle]
pub extern "C" fn elephc_mbstring_substitute_character_set_v1(code: i64) -> i32 {
    catch_unwind(AssertUnwindSafe(|| {
        i32::from(super::REQUEST.with(|state| state.borrow_mut().set_substitute_codepoint(code).is_ok()))
    }))
    .unwrap_or(0)
}

/// Applies regex options and returns the byte length of the previous option string, or `-1`.
#[no_mangle]
pub unsafe extern "C" fn elephc_mbstring_regex_set_options_len_v1(
    options: *const u8,
    len: u64,
) -> i64 {
    catch_unwind(AssertUnwindSafe(|| unsafe {
        if len > isize::MAX as u64 || (len != 0 && options.is_null()) {
            return -1;
        }
        let bytes = if len == 0 {
            &[][..]
        } else {
            std::slice::from_raw_parts(options, len as usize)
        };
        super::REGEX.with(|session| {
            session
                .set_options(bytes)
                .map(|previous| previous.as_string().len() as i64)
                .unwrap_or(-1)
        })
    }))
    .unwrap_or(-1)
}

/// Returns the byte length of the identified HTTP input encoding, or `-1` for unsupported lists.
#[no_mangle]
pub unsafe extern "C" fn elephc_mbstring_http_input_len_v1(
    selector: *const u8,
    len: u64,
) -> i64 {
    catch_unwind(AssertUnwindSafe(|| unsafe {
        if len > isize::MAX as u64 || (len != 0 && selector.is_null()) {
            return -1;
        }
        let bytes = if len == 0 {
            &[][..]
        } else {
            std::slice::from_raw_parts(selector, len as usize)
        };
        match super::REQUEST.with(|state| state.borrow().http_input(Some(bytes))) {
            Ok(crate::state::InputInformation::Unidentified) => 0,
            Ok(crate::state::InputInformation::String(name)) => name.len() as i64,
            Ok(crate::state::InputInformation::List(_)) | Err(_) => -1,
        }
    }))
    .unwrap_or(-1)
}

/// Sets the detection order from one validated UTF-8 array element.
///
/// The already-UTF-8 case skips list parsing and allocation, which is the common hot path for
/// request-local code that repeatedly asserts its default detection order.
///
/// # Safety
/// `name` must be null or point to `len` readable bytes for the duration of the call.
#[no_mangle]
pub unsafe extern "C" fn elephc_mbstring_detect_order_single_v1(
    name: *const u8,
    len: u64,
) -> i32 {
    catch_unwind(AssertUnwindSafe(|| unsafe {
        if len > isize::MAX as u64 || (len != 0 && name.is_null()) {
            return 0;
        }
        let bytes = if len == 0 {
            &[][..]
        } else {
            std::slice::from_raw_parts(name, len as usize)
        };
        super::REQUEST.with(|state| {
            let mut state = state.borrow_mut();
            if bytes.eq_ignore_ascii_case(b"UTF-8")
                && state.detect_order().len() == 1
                && state.detect_order()[0].name().eq_ignore_ascii_case("UTF-8")
            {
                return 1;
            }
            let list = [bytes.to_vec()];
            i32::from(state.set_detect_order(EncodingList::Array(&list)).is_ok())
        })
    }))
    .unwrap_or(0)
}

/// Reports whether the request converts UTF-8 to UTF-8, where `mb_output_handler()` is a copy.
#[no_mangle]
pub extern "C" fn elephc_mbstring_output_utf8_passthrough_v1() -> i32 {
    catch_unwind(AssertUnwindSafe(|| {
        super::REQUEST.with(|state| {
            let state = state.borrow();
            i32::from(
                state.http_output().name() == "UTF-8" && state.internal_encoding().name() == "UTF-8",
            )
        })
    }))
    .unwrap_or(0)
}

/// Returns the number of canonical encodings exposed by `mb_list_encodings()`.
#[no_mangle]
pub extern "C" fn elephc_mbstring_list_encodings_count_v1() -> i64 {
    catch_unwind(AssertUnwindSafe(|| Encoding::all().count() as i64)).unwrap_or(-1)
}
