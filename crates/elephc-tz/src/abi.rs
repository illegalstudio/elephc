//! Purpose:
//! The C ABI the elephc `tz_prelude` calls into. Each function reads a zone name
//! (pointer + length) and returns a serialized, NUL-terminated string into a
//! per-function static buffer that the PHP marshalling parses into the
//! getLocation/getTransitions/listAbbreviations arrays.
//!
//! Called from:
//! - Compiled PHP program assembly through the `extern "elephc_tz"` block.
//! - `cargo test -p elephc-tz` (the rlib) exercises the serialization directly.
//!
//! Key details:
//! - The returned pointer is owned by a `static Mutex<CString>` and stays valid
//!   until the next call to the same function. The compiled PHP program is
//!   single-threaded and copies the bytes immediately, mirroring the PDO bridge.
//! - An empty return marks "no data" (a false-zone or unknown name), since every
//!   present location/transition serialization is non-empty.

use std::borrow::Cow;
use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::sync::{Mutex, OnceLock};

use crate::{abbreviations, zone_location, zone_transitions};

/// Reads a borrowed zone name from a NUL-terminated C string — the way elephc
/// lowers an extern `string` argument (a single `char*`). A null pointer yields
/// `""`, and invalid UTF-8 simply fails to match a zone.
unsafe fn zone_name<'a>(ptr: *const c_char) -> Cow<'a, str> {
    if ptr.is_null() {
        Cow::Borrowed("")
    } else {
        CStr::from_ptr(ptr).to_string_lossy()
    }
}

/// Moves `s` into `cell` and returns a pointer to its NUL-terminated bytes. The
/// pointer is valid until the next call that stashes into the same cell. A
/// `String` carrying an interior NUL (never produced here) degrades to empty.
fn stash(cell: &'static Mutex<CString>, s: String) -> *const c_char {
    let value = CString::new(s).unwrap_or_default();
    let mut guard = cell.lock().expect("tz bridge buffer mutex poisoned");
    *guard = value;
    guard.as_ptr()
}

/// Serializes one zone's transitions as `ts\toffset\tdst\tabbr\ttime` rows joined
/// by `\n`, or the empty string for a false-zone/unknown name.
fn serialize_transitions(name: &str) -> String {
    let Some(rows) = zone_transitions(name) else {
        return String::new();
    };
    let mut out = String::new();
    for (i, r) in rows.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        // ts \t offset \t dst(0/1) \t abbr \t time
        out.push_str(&r.ts.to_string());
        out.push('\t');
        out.push_str(&r.offset.to_string());
        out.push('\t');
        out.push(if r.isdst { '1' } else { '0' });
        out.push('\t');
        out.push_str(&r.abbr);
        out.push('\t');
        out.push_str(&r.time);
    }
    out
}

/// Serializes a zone's location as `cc\tlat\tlon\tcomments`, or the empty string
/// for a false-zone/unknown name (`cc` is always non-empty when present, so empty
/// is unambiguous).
fn serialize_location(name: &str) -> String {
    match zone_location(name) {
        Some(loc) => format!(
            "{}\t{}\t{}\t{}",
            loc.country_code, loc.latitude, loc.longitude, loc.comments
        ),
        None => String::new(),
    }
}

/// Serializes the full abbreviation table as `abbr\t<dst>:<off>:<id>;...` lines
/// (one per abbreviation, in PHP order; an empty id means null).
fn serialize_abbreviations() -> String {
    let mut out = String::new();
    for (i, (abbr, rows)) in abbreviations().iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        out.push_str(abbr);
        out.push('\t');
        for (j, row) in rows.iter().enumerate() {
            if j > 0 {
                out.push(';');
            }
            out.push(if row.dst { '1' } else { '0' });
            out.push(':');
            out.push_str(&row.offset.to_string());
            out.push(':');
            // A literal "NULL" (never a real timezone id) marks PHP's null
            // timezone_id. An empty trailing field would be fragile to parse in the
            // 1127-row marshalling loop, so the field is always non-empty.
            out.push_str(row.timezone_id.unwrap_or("NULL"));
        }
    }
    out
}

/// Returns the process-wide buffer cell for transition results.
fn transitions_cell() -> &'static Mutex<CString> {
    static CELL: OnceLock<Mutex<CString>> = OnceLock::new();
    CELL.get_or_init(|| Mutex::new(CString::default()))
}

/// Returns the process-wide buffer cell for location results.
fn location_cell() -> &'static Mutex<CString> {
    static CELL: OnceLock<Mutex<CString>> = OnceLock::new();
    CELL.get_or_init(|| Mutex::new(CString::default()))
}

/// Returns the process-wide buffer cell for the abbreviation table.
fn abbreviations_cell() -> &'static Mutex<CString> {
    static CELL: OnceLock<Mutex<CString>> = OnceLock::new();
    CELL.get_or_init(|| Mutex::new(CString::default()))
}

/// C ABI: returns a zone's `getTransitions()` rows serialized as
/// `ts\toffset\tdst\tabbr\ttime` lines, or an empty string for a false-zone or
/// unknown name (which the marshalling turns into PHP `false`).
///
/// # Safety
/// `name` must be a valid NUL-terminated C string, or null.
#[no_mangle]
pub unsafe extern "C" fn elephc_tz_transitions(name: *const c_char) -> *const c_char {
    let name = zone_name(name);
    stash(transitions_cell(), serialize_transitions(&name))
}

/// C ABI: returns a zone's `getLocation()` data serialized as
/// `cc\tlat\tlon\tcomments`, or an empty string for a false-zone or unknown name.
///
/// # Safety
/// `name` must be a valid NUL-terminated C string, or null.
#[no_mangle]
pub unsafe extern "C" fn elephc_tz_location(name: *const c_char) -> *const c_char {
    let name = zone_name(name);
    stash(location_cell(), serialize_location(&name))
}

/// C ABI: returns the whole `listAbbreviations()` table serialized as
/// `abbr\t<dst>:<off>:<id>;...` lines in PHP order. Takes no argument.
#[no_mangle]
pub extern "C" fn elephc_tz_abbreviations() -> *const c_char {
    stash(abbreviations_cell(), serialize_abbreviations())
}

#[cfg(test)]
mod tests {
    //! Purpose:
    //! Validates the C-ABI serialization shape against the parsed tables so the
    //! PHP-side marshalling has a stable, parseable contract.
    //!
    //! Called from:
    //! - `cargo test -p elephc-tz` through Rust's test harness.
    //!
    //! Key details:
    //! - Exercises the serialize_* helpers directly (the `extern "C"` wrappers add
    //!   only pointer plumbing over them).

    use super::*;

    /// Paris transitions serialize to 185 newline-joined rows of 5 tab fields each,
    /// with the synthetic LMT row first.
    #[test]
    fn serializes_transitions() {
        let s = serialize_transitions("Europe/Paris");
        let lines: Vec<&str> = s.split('\n').collect();
        assert_eq!(lines.len(), 185);
        let first: Vec<&str> = lines[0].split('\t').collect();
        assert_eq!(first.len(), 5);
        assert_eq!(first[0], "-9223372036854775808");
        assert_eq!(first[3], "LMT");
        assert_eq!(first[4], "-292277022657-01-27T08:29:52+00:00");
    }

    /// A false-zone serializes to empty (decoded as PHP `false`).
    #[test]
    fn serializes_false_zone_as_empty() {
        assert!(serialize_transitions("CET").is_empty());
        assert!(serialize_location("CET").is_empty());
    }

    /// Location serializes the four tab-separated fields, including the special
    /// `UTC` values.
    #[test]
    fn serializes_location() {
        assert_eq!(
            serialize_location("Europe/Paris"),
            "FR\t48.866659999999996\t2.3333299999999895\t"
        );
        assert_eq!(serialize_location("UTC"), "??\t-90\t-180\t");
    }

    /// The abbreviation serialization yields 144 lines in PHP order, and a null
    /// timezone_id is emitted as the non-empty `NULL` marker (never a trailing
    /// empty field).
    #[test]
    fn serializes_abbreviations() {
        let s = serialize_abbreviations();
        let lines: Vec<&str> = s.split('\n').collect();
        assert_eq!(lines.len(), 144);
        assert!(lines[0].starts_with("acdt\t1:37800:Australia/Adelaide"));
        // The "a" military zone has a null timezone_id; it must serialize as `:NULL`,
        // not a trailing `:`.
        let a_line = lines.iter().find(|l| l.starts_with("a\t")).expect("abbr 'a'");
        assert!(a_line.contains(":NULL"), "null id must use the NULL marker: {a_line}");
        assert!(!a_line.ends_with(':'), "no trailing empty field: {a_line}");
    }
}
