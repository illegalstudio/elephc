//! Purpose:
//! PHP's libxml error state (`ext/libxml/libxml.c`): the `libxml_use_internal_errors()` mode,
//! the error list `libxml_get_errors()` returns, and the last error
//! `libxml_get_last_error()` falls back to, behind a panic-free handle-free C ABI.
//!
//! Called from:
//! - The compiler's libxml prelude (`libxml_*` functions and `LibXMLError`) through the
//!   `extern "elephc_xml"` block.
//! - `crate::simplexml` diagnostics after every libxml2 operation, through [`absorb`].
//!
//! Key details:
//! - The shim records every libxml2 error of the thread (`elephc_libxml2_v1_take_error_records`).
//!   Errors `LIBXML_NOERROR`/`LIBXML_NOWARNING` hide from the warnings still join the list, as
//!   PHP's structured handler receives them.
//! - With internal errors on, [`absorb`] returns `true` so callers drop their warning text:
//!   php-src's structured handler replaces the warnings.
//! - Disabling internal errors destroys the list; `libxml_get_errors()` is then `[]`, and
//!   `libxml_get_last_error()` reads the last error instead of the list's last entry.

use std::cell::RefCell;
use std::ffi::{c_char, c_void, CStr, CString};
use std::panic::{catch_unwind, AssertUnwindSafe};

extern "C" {
    /// Moves the thread's recorded libxml2 errors out of the shim (NULL when there are none).
    #[link_name = "elephc_libxml2_v1_take_error_records"]
    fn take_error_records() -> *mut c_char;
    /// Frees a string the shim allocated with libxml2's allocator.
    #[link_name = "elephc_libxml2_v1_sx_string_free"]
    fn string_free(value: *mut c_void);
}

/// One libxml2 error, as `LibXMLError` exposes it.
#[derive(Clone, Default, Debug, PartialEq)]
pub(crate) struct Record {
    pub(crate) level: i64,
    pub(crate) code: i64,
    pub(crate) column: i64,
    pub(crate) line: i64,
    pub(crate) file: Vec<u8>,
    pub(crate) message: Vec<u8>,
}

#[derive(Default)]
struct State {
    /// The error list; `Some` exactly while internal errors are enabled.
    list: Option<Vec<Record>>,
    /// libxml2's last error, updated by every error including filtered ones.
    last: Option<Record>,
    /// The error the next field reads come from.
    selected: Option<Record>,
    /// Keeps the last returned string alive until the next string read.
    output: CString,
}

thread_local! { static STATE: RefCell<State> = RefCell::new(State::default()); }

/// Runs an ABI operation without allowing Rust panics to unwind into generated code.
fn guard<T: Default>(body: impl FnOnce() -> T) -> T {
    catch_unwind(AssertUnwindSafe(body)).unwrap_or_default()
}

/// Splits the shim's record buffer into records.
fn parse_records(raw: &[u8]) -> Vec<Record> {
    raw.split(|byte| *byte == 0x1e)
        .filter(|record| !record.is_empty())
        .filter_map(|record| {
            let fields: Vec<&[u8]> = record.splitn(6, |byte| *byte == 0x1f).collect();
            let [level, code, column, line, file, message] = fields.as_slice() else {
                return None;
            };
            let int = |field: &[u8]| std::str::from_utf8(field).ok()?.parse::<i64>().ok();
            Some(Record {
                level: int(level)?,
                code: int(code)?,
                column: int(column)?,
                line: int(line)?,
                file: file.to_vec(),
                message: message.to_vec(),
            })
        })
        .collect()
}

/// Folds records into the state; returns whether internal errors are on.
fn record(records: Vec<Record>) -> bool {
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        for error in records {
            if let Some(list) = state.list.as_mut() {
                list.push(error.clone());
            }
            state.last = Some(error);
        }
        state.list.is_some()
    })
}

/// Drains the shim's errors after a libxml2 operation into PHP's error state.
///
/// Returns `true` when internal errors are enabled, meaning the operation's warning text
/// must not be emitted.
pub(crate) fn absorb() -> bool {
    let raw = unsafe { take_error_records() };
    let records = if raw.is_null() {
        Vec::new()
    } else {
        let bytes = unsafe { CStr::from_ptr(raw) }.to_bytes().to_vec();
        unsafe { string_free(raw.cast()) };
        parse_records(&bytes)
    };
    record(records)
}

/// `libxml_use_internal_errors()`: `mode` is -1 to query, 0 to disable, 1 to enable.
/// Returns the previous mode.
#[no_mangle]
pub extern "C" fn elephc_xml_libxml_use_internal_errors(mode: i64) -> i64 {
    guard(|| {
        STATE.with(|state| {
            let mut state = state.borrow_mut();
            let previous = state.list.is_some();
            match mode {
                0 => state.list = None,
                1 if state.list.is_none() => state.list = Some(Vec::new()),
                _ => {}
            }
            i64::from(previous)
        })
    })
}

/// Number of errors in the list (zero when internal errors are disabled).
#[no_mangle]
pub extern "C" fn elephc_xml_libxml_error_count() -> i64 {
    guard(|| STATE.with(|state| state.borrow().list.as_ref().map_or(0, |list| list.len() as i64)))
}

/// Selects the error later field reads return: list entry `index`, or for `-1` the error
/// `libxml_get_last_error()` reports (the list's last entry, else libxml2's last error).
/// Returns 1 when such an error exists.
#[no_mangle]
pub extern "C" fn elephc_xml_libxml_error_select(index: i64) -> i64 {
    guard(|| {
        STATE.with(|state| {
            let mut state = state.borrow_mut();
            let selected = match (index, state.list.as_ref()) {
                (-1, Some(list)) => list.last().cloned(),
                (-1, None) => state.last.clone(),
                (index, Some(list)) if index >= 0 => list.get(index as usize).cloned(),
                _ => None,
            };
            let found = selected.is_some();
            state.selected = selected;
            i64::from(found)
        })
    })
}

/// Integer field of the selected error: 0 level, 1 code, 2 column, 3 line.
#[no_mangle]
pub extern "C" fn elephc_xml_libxml_error_int(field: i64) -> i64 {
    guard(|| {
        STATE.with(|state| {
            let state = state.borrow();
            let Some(error) = state.selected.as_ref() else { return 0 };
            match field {
                0 => error.level,
                1 => error.code,
                2 => error.column,
                3 => error.line,
                _ => 0,
            }
        })
    })
}

/// String field of the selected error: 0 message, 1 file. Valid until the next string read.
#[no_mangle]
pub extern "C" fn elephc_xml_libxml_error_string(field: i64) -> *const c_char {
    guard(|| {
        STATE.with(|state| {
            let mut state = state.borrow_mut();
            let value = match (state.selected.as_ref(), field) {
                (Some(error), 0) => error.message.clone(),
                (Some(error), 1) => error.file.clone(),
                _ => Vec::new(),
            };
            state.output = CString::new(value).unwrap_or_default();
            state.output.as_ptr()
        })
    })
}

/// `libxml_clear_errors()`: resets the last error and empties the list.
#[no_mangle]
pub extern "C" fn elephc_xml_libxml_clear_errors() {
    guard(|| {
        STATE.with(|state| {
            let mut state = state.borrow_mut();
            state.last = None;
            if let Some(list) = state.list.as_mut() {
                list.clear();
            }
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Resets the thread's state so tests do not observe each other.
    fn reset() {
        STATE.with(|state| *state.borrow_mut() = State::default());
    }

    fn error(message: &str) -> Record {
        Record { level: 3, code: 76, column: 5, line: 2, file: b"a.xml".to_vec(), message: message.into() }
    }

    /// The shim's record format round-trips, including empty file names and separators.
    #[test]
    fn records_parse_from_the_shim_format() {
        let raw = b"3\x1f76\x1f5\x1f2\x1fa.xml\x1fbad\n\x1e1\x1f9\x1f0\x1f0\x1f\x1fw\x1e";
        let records = parse_records(raw);
        assert_eq!(records.len(), 2);
        assert_eq!(records[0], error("bad\n"));
        assert_eq!(records[1].file, b"");
    }

    /// Errors only reach the list in internal-errors mode, the last error is kept either
    /// way, and disabling destroys the list.
    #[test]
    fn list_and_last_error_follow_php_libxml_rules() {
        reset();
        assert!(!record(vec![error("a")]));
        assert_eq!(elephc_xml_libxml_error_count(), 0);
        assert_eq!(elephc_xml_libxml_error_select(-1), 1);
        assert_eq!(elephc_xml_libxml_use_internal_errors(1), 0);
        assert!(record(vec![error("b"), error("c")]));
        assert_eq!(elephc_xml_libxml_error_count(), 2);
        assert_eq!(elephc_xml_libxml_error_select(-1), 1);
        assert_eq!(elephc_xml_libxml_error_int(1), 76);
        let message = unsafe { CStr::from_ptr(elephc_xml_libxml_error_string(0)) };
        assert_eq!(message.to_bytes(), b"c");
        assert_eq!(elephc_xml_libxml_use_internal_errors(-1), 1);
        assert_eq!(elephc_xml_libxml_use_internal_errors(0), 1);
        assert_eq!(elephc_xml_libxml_error_count(), 0);
        elephc_xml_libxml_clear_errors();
        assert_eq!(elephc_xml_libxml_error_select(-1), 0);
    }
}
