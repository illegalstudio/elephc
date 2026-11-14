//! Purpose:
//! C ABI entry points for libtidy configuration options: enumeration, lookup,
//! typed reads, typed writes, and option help text.
//!
//! Called from:
//! - Compiled PHP programs through the `extern "elephc_tidy"` block declared by
//!   `elephc::tidy_prelude`.
//!
//! Key details:
//! - Option pointers are cached per document in `tidyGetOptionList` order, which
//!   is the same order php-src's `tidy_get_config()` exposes.
//! - `-1` means an unknown option index/name and `-2` means a libtidy-internal
//!   option that user code may read but not set. The prelude turns those into
//!   PHP's `ValueError` messages.
//! - Write helpers pass PHP's integer through unchanged; the prelude owns the
//!   PHP coercion rules and reports libtidy's rejection.

use std::ffi::{c_char, c_int, c_ulong, CString};

use crate::abi::{c_bytes, flag, guarded, stash, OPTION_DOC, OPTION_NAME, OPTION_VALUE};
use crate::ffi::{
    tidyGetOptionByName, tidyOptGetBool, tidyOptGetCategory, tidyOptGetDoc, tidyOptGetId,
    tidyOptGetInt, tidyOptGetName, tidyOptGetType, tidyOptGetValue, tidyOptSetBool, tidyOptSetInt,
    tidyOptSetValue, TidyOption, OPTION_CATEGORY_INTERNAL, OPTION_TYPE_BOOLEAN,
    OPTION_TYPE_INTEGER, OPTION_TYPE_STRING,
};
use crate::registry::{options, with_doc};

/// Value returned for an unknown option.
const OPTION_UNKNOWN: i64 = -1;

/// Reads a PHP string argument up to its first NUL, matching libtidy's C API.
fn nul_terminated(bytes: &[u8]) -> Vec<u8> {
    bytes
        .iter()
        .position(|byte| *byte == 0)
        .map_or_else(|| bytes.to_vec(), |end| bytes[..end].to_vec())
}

/// Returns the option pointer at `index` for one document.
fn option_at(entry: &mut crate::registry::DocEntry, index: i64) -> Option<*mut TidyOption> {
    if index < 0 {
        return None;
    }
    let pointer = *options(entry).get(index as usize)? as *mut TidyOption;
    (!pointer.is_null()).then_some(pointer)
}

/// C ABI: returns the number of configuration options in iterator order.
#[no_mangle]
pub extern "C" fn elephc_tidy_option_count(handle: i64) -> i64 {
    guarded(0, || with_doc(handle, 0, |entry, _| options(entry).len() as i64))
}

/// C ABI: returns the iterator index of a named option, or -1 when unknown.
///
/// # Safety
/// `name` must be a NUL-terminated string.
#[no_mangle]
pub unsafe extern "C" fn elephc_tidy_option_index(
    handle: i64,
    name: *const c_char,
) -> i64 {
    let bytes = nul_terminated(&c_bytes(name));
    guarded(OPTION_UNKNOWN, || {
        with_doc(handle, OPTION_UNKNOWN, |entry, doc| {
            let Ok(name) = CString::new(bytes) else {
                return OPTION_UNKNOWN;
            };
            let target = tidyGetOptionByName(doc, name.as_ptr());
            if target.is_null() {
                return OPTION_UNKNOWN;
            }
            options(entry)
                .iter()
                .position(|option| *option as *mut TidyOption == target)
                .map_or(OPTION_UNKNOWN, |index| index as i64)
        })
    })
}

/// C ABI: returns an option's canonical name, or an empty string for a bad index.
#[no_mangle]
pub extern "C" fn elephc_tidy_option_name(handle: i64, index: i64) -> *const c_char {
    let bytes = guarded(Vec::new(), || {
        with_doc(handle, Vec::new(), |entry, _| {
            let Some(option) = option_at(entry, index) else {
                return Vec::new();
            };
            unsafe { c_bytes(tidyOptGetName(option)) }
        })
    });
    stash(&OPTION_NAME, bytes)
}

/// C ABI: returns an option's type: 0 string, 1 integer, 2 boolean, -1 invalid.
#[no_mangle]
pub extern "C" fn elephc_tidy_option_type(handle: i64, index: i64) -> i64 {
    guarded(OPTION_UNKNOWN, || {
        with_doc(handle, OPTION_UNKNOWN, |entry, _| {
            let Some(option) = option_at(entry, index) else {
                return OPTION_UNKNOWN;
            };
            match unsafe { tidyOptGetType(option) } {
                OPTION_TYPE_STRING => 0,
                OPTION_TYPE_INTEGER => 1,
                OPTION_TYPE_BOOLEAN => 2,
                _ => OPTION_UNKNOWN,
            }
        })
    })
}

/// C ABI: returns whether an option is libtidy-internal (read-only for users).
#[no_mangle]
pub extern "C" fn elephc_tidy_option_internal(handle: i64, index: i64) -> i64 {
    guarded(0, || {
        with_doc(handle, 0, |entry, _| {
            let Some(option) = option_at(entry, index) else {
                return 0;
            };
            flag(unsafe { tidyOptGetCategory(option) } == OPTION_CATEGORY_INTERNAL)
        })
    })
}

/// C ABI: returns a string option's current value.
#[no_mangle]
pub extern "C" fn elephc_tidy_option_value_str(handle: i64, index: i64) -> *const c_char {
    let bytes = guarded(Vec::new(), || {
        with_doc(handle, Vec::new(), |entry, doc| {
            let Some(option) = option_at(entry, index) else {
                return Vec::new();
            };
            let id = unsafe { tidyOptGetId(option) };
            unsafe { c_bytes(tidyOptGetValue(doc, id)) }
        })
    });
    stash(&OPTION_VALUE, bytes)
}

/// C ABI: returns an integer option's current value.
#[no_mangle]
pub extern "C" fn elephc_tidy_option_value_int(handle: i64, index: i64) -> i64 {
    guarded(0, || {
        with_doc(handle, 0, |entry, doc| {
            let Some(option) = option_at(entry, index) else {
                return 0;
            };
            let id = unsafe { tidyOptGetId(option) };
            unsafe { tidyOptGetInt(doc, id) as i64 }
        })
    })
}

/// C ABI: returns a boolean option's current value as 1/0.
#[no_mangle]
pub extern "C" fn elephc_tidy_option_value_bool(handle: i64, index: i64) -> i64 {
    guarded(0, || {
        with_doc(handle, 0, |entry, doc| {
            let Some(option) = option_at(entry, index) else {
                return 0;
            };
            let id = unsafe { tidyOptGetId(option) };
            flag(unsafe { tidyOptGetBool(doc, id) } != 0)
        })
    })
}

/// C ABI: returns an option's help text, or an empty string when it has none.
#[no_mangle]
pub extern "C" fn elephc_tidy_option_doc(handle: i64, index: i64) -> *const c_char {
    let bytes = guarded(Vec::new(), || {
        with_doc(handle, Vec::new(), |entry, doc| {
            let Some(option) = option_at(entry, index) else {
                return Vec::new();
            };
            unsafe { c_bytes(tidyOptGetDoc(doc, option)) }
        })
    });
    stash(&OPTION_DOC, bytes)
}

/// C ABI: returns whether libtidy supplied help text for an option.
///
/// php-src distinguishes a NULL documentation pointer (returns `false`) from an
/// empty documentation string (returns `""`); the string entry point alone cannot
/// tell those apart.
#[no_mangle]
pub extern "C" fn elephc_tidy_option_doc_present(handle: i64, index: i64) -> i64 {
    guarded(0, || {
        with_doc(handle, 0, |entry, doc| {
            let Some(option) = option_at(entry, index) else {
                return 0;
            };
            flag(!unsafe { tidyOptGetDoc(doc, option) }.is_null())
        })
    })
}

/// C ABI: sets a string option value; returns 1 on success.
///
/// # Safety
/// `name` and `value` must be NUL-terminated strings.
#[no_mangle]
pub unsafe extern "C" fn elephc_tidy_set_option_str(
    handle: i64,
    name: *const c_char,
    value: *const c_char,
) -> i64 {
    let name = nul_terminated(&c_bytes(name));
    let value = nul_terminated(&c_bytes(value));
    guarded(0, || {
        with_doc(handle, 0, |_, doc| {
            let (Ok(name), Ok(value)) = (CString::new(name), CString::new(value)) else {
                return 0;
            };
            let option = tidyGetOptionByName(doc, name.as_ptr());
            if option.is_null() {
                return 0;
            }
            let id = tidyOptGetId(option);
            flag(tidyOptSetValue(doc, id, value.as_ptr()) != 0)
        })
    })
}

/// C ABI: sets an integer option value; returns 1 on success.
///
/// # Safety
/// `name` must be a NUL-terminated string.
#[no_mangle]
pub unsafe extern "C" fn elephc_tidy_set_option_int(
    handle: i64,
    name: *const c_char,
    value: i64,
) -> i64 {
    let name = nul_terminated(&c_bytes(name));
    guarded(0, || {
        with_doc(handle, 0, |_, doc| {
            let Ok(name) = CString::new(name) else {
                return 0;
            };
            let option = tidyGetOptionByName(doc, name.as_ptr());
            if option.is_null() {
                return 0;
            }
            let id = tidyOptGetId(option);
            flag(tidyOptSetInt(doc, id, value as c_ulong) != 0)
        })
    })
}

/// C ABI: sets a boolean option value; returns 1 on success.
///
/// # Safety
/// `name` must be a NUL-terminated string.
#[no_mangle]
pub unsafe extern "C" fn elephc_tidy_set_option_bool(
    handle: i64,
    name: *const c_char,
    value: i64,
) -> i64 {
    let name = nul_terminated(&c_bytes(name));
    guarded(0, || {
        with_doc(handle, 0, |_, doc| {
            let Ok(name) = CString::new(name) else {
                return 0;
            };
            let option = tidyGetOptionByName(doc, name.as_ptr());
            if option.is_null() {
                return 0;
            }
            let id = tidyOptGetId(option);
            flag(tidyOptSetBool(doc, id, value as c_int) != 0)
        })
    })
}

#[cfg(test)]
mod tests {
    //! Purpose:
    //! Pure helpers for the option C ABI.
    //!
    //! Called from:
    //! - `cargo test` through Rust's test harness.
    //!
    //! Key details:
    //! - Mirrors libtidy's C-string treatment without calling it.

    use super::*;

    /// A PHP string is cut at its first interior NUL.
    #[test]
    fn nul_terminated_matches_c_string_semantics() {
        assert_eq!(nul_terminated(b"blocklevel"), b"blocklevel");
        assert_eq!(nul_terminated(b"block\0level"), b"block");
        assert!(nul_terminated(b"").is_empty());
    }
}
