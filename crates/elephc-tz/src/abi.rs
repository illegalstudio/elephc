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
//! - Every export contains Rust panics at the ABI boundary. String exports return
//!   a stable empty C string on failure, the offset export returns `i64::MIN`, and
//!   poisoned result-buffer mutexes are recovered instead of propagated.
//! - `elephc_tz_offset` is a separate, windows-only offset resolver (not part of
//!   the introspection surface above): it is published into a runtime
//!   function-pointer slot (Mechanism A, mirroring elephc-crypto's `hash()`
//!   entry points) rather than declared through the `extern "elephc_tz"` PHP
//!   block the introspection methods use (Mechanism B), because its callers are
//!   hand-written `__rt_sys_localtime`/`__rt_sys_mktime` runtime helpers, not PHP
//!   source. See `crate::codegen_support::tz_bridge` in the main crate.
//! - `elephc_tz_abbreviation` complements that scalar bridge with a pointer into
//!   its own stable mutex-backed cell, valid until the next abbreviation lookup.

use std::borrow::Cow;
use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::{Mutex, OnceLock};

use crate::{abbreviations, zone_location, zone_offset_at, zone_transitions};

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
    let mut guard = cell.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    *guard = value;
    guard.as_ptr()
}

/// Runs one string-returning C export behind an unwind barrier and stores its
/// result in the export's stable buffer. A panic becomes a valid empty C string,
/// matching the existing false-zone/error sentinel instead of crossing the ABI.
fn catch_string_export(
    cell: &'static Mutex<CString>,
    body: impl FnOnce() -> String,
) -> *const c_char {
    let value = catch_unwind(AssertUnwindSafe(body)).unwrap_or_default();
    stash(cell, value)
}

/// Runs one scalar C export behind an unwind barrier. A panic becomes the
/// supplied sentinel so Rust unwinding never reaches foreign callers.
fn catch_scalar_export(body: impl FnOnce() -> i64, sentinel: i64) -> i64 {
    catch_unwind(AssertUnwindSafe(body)).unwrap_or(sentinel)
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

/// Returns the process-wide buffer cell for one transition abbreviation.
fn abbreviation_cell() -> &'static Mutex<CString> {
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
    catch_string_export(transitions_cell(), || {
        let name = zone_name(name);
        serialize_transitions(&name)
    })
}

/// C ABI: returns a zone's `getLocation()` data serialized as
/// `cc\tlat\tlon\tcomments`, or an empty string for a false-zone or unknown name.
///
/// # Safety
/// `name` must be a valid NUL-terminated C string, or null.
#[no_mangle]
pub unsafe extern "C" fn elephc_tz_location(name: *const c_char) -> *const c_char {
    catch_string_export(location_cell(), || {
        let name = zone_name(name);
        serialize_location(&name)
    })
}

/// C ABI: returns the whole `listAbbreviations()` table serialized as
/// `abbr\t<dst>:<off>:<id>;...` lines in PHP order. Takes no argument.
#[no_mangle]
pub extern "C" fn elephc_tz_abbreviations() -> *const c_char {
    catch_string_export(abbreviations_cell(), serialize_abbreviations)
}

/// C ABI: resolves `name`'s UTC offset (and DST flag) at Unix timestamp `ts` via
/// [`crate::zone_offset_at`], for the windows-only local-time bridge. Packs both
/// fields into one `i64`: `offset_seconds * 2 + (is_dst as i64)`, recoverable as
/// `let isdst = packed & 1; let offset = (packed - isdst) >> 1;` — exact, since
/// `packed - isdst` is always even and a real UTC offset is a small, exactly
/// representable multiple of a minute. Returns `i64::MIN` — never a legitimate
/// packed value, since real offsets are many orders of magnitude smaller — when
/// `name` is unknown or a false-zone (no transition data), telling the caller to
/// fall back to its own (non-bridge) offset resolution.
///
/// # Safety
/// `name` must be a valid NUL-terminated C string, or null.
#[no_mangle]
pub unsafe extern "C" fn elephc_tz_offset(name: *const c_char, ts: i64) -> i64 {
    catch_scalar_export(
        || {
            let name = zone_name(name);
            match zone_offset_at(&name, ts) {
                Some((offset, isdst, _abbr)) => (offset as i64) * 2 + i64::from(isdst),
                None => i64::MIN,
            }
        },
        i64::MIN,
    )
}

/// C ABI: resolves the active transition abbreviation for `name` at `ts`.
///
/// The returned NUL-terminated pointer remains valid until the next call to
/// this function. Unknown and false zones return a stable empty string.
///
/// # Safety
/// `name` must be a valid NUL-terminated C string, or null.
#[no_mangle]
pub unsafe extern "C" fn elephc_tz_abbreviation(
    name: *const c_char,
    ts: i64,
) -> *const c_char {
    catch_string_export(abbreviation_cell(), || {
        let name = zone_name(name);
        zone_offset_at(&name, ts)
            .map(|(_, _, abbreviation)| abbreviation)
            .unwrap_or_default()
    })
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

    /// The C string exports expose the same non-null empty sentinel for unknown
    /// transition and location zones, including a null input pointer.
    #[test]
    fn string_exports_use_valid_empty_sentinel() {
        let unknown = CString::new("Not/AZone").unwrap();
        let transitions = unsafe { elephc_tz_transitions(unknown.as_ptr()) };
        assert!(!transitions.is_null());
        assert_eq!(unsafe { CStr::from_ptr(transitions) }.to_bytes(), b"");

        let location = unsafe { elephc_tz_location(std::ptr::null()) };
        assert!(!location.is_null());
        assert_eq!(unsafe { CStr::from_ptr(location) }.to_bytes(), b"");

        let abbreviations = elephc_tz_abbreviations();
        assert!(!abbreviations.is_null());
        assert!(!unsafe { CStr::from_ptr(abbreviations) }.to_bytes().is_empty());
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

    /// Decodes an `elephc_tz_offset` packed `i64` back into `(offset, isdst)`, the
    /// way a caller (the windows runtime bridge) is expected to: mirrors the
    /// scheme documented on `elephc_tz_offset` itself.
    fn unpack(packed: i64) -> (i64, bool) {
        let isdst = packed & 1;
        ((packed - isdst) >> 1, isdst != 0)
    }

    /// `elephc_tz_offset` round-trips through a real NUL-terminated C string: a
    /// winter Europe/Paris instant packs to `(3600, false)`.
    #[test]
    fn elephc_tz_offset_resolves_known_zone() {
        let name = CString::new("Europe/Paris").unwrap();
        let packed = unsafe { elephc_tz_offset(name.as_ptr(), 1_705_320_000) };
        assert_eq!(unpack(packed), (3600, false));
    }

    /// A summer Europe/Paris instant packs to `(7200, true)` (DST/CEST).
    #[test]
    fn elephc_tz_offset_resolves_dst() {
        let name = CString::new("Europe/Paris").unwrap();
        let packed = unsafe { elephc_tz_offset(name.as_ptr(), 1_721_044_800) };
        assert_eq!(unpack(packed), (7200, true));
    }

    /// The abbreviation bridge returns transition-specific stable C strings.
    #[test]
    fn elephc_tz_abbreviation_resolves_winter_summer_and_utc() {
        let paris = CString::new("Europe/Paris").unwrap();
        let winter = unsafe { elephc_tz_abbreviation(paris.as_ptr(), 1_705_320_000) };
        assert_eq!(unsafe { CStr::from_ptr(winter) }.to_bytes(), b"CET");
        let summer = unsafe { elephc_tz_abbreviation(paris.as_ptr(), 1_721_044_800) };
        assert_eq!(unsafe { CStr::from_ptr(summer) }.to_bytes(), b"CEST");
        let utc = CString::new("UTC").unwrap();
        let utc_abbreviation = unsafe { elephc_tz_abbreviation(utc.as_ptr(), 0) };
        assert_eq!(unsafe { CStr::from_ptr(utc_abbreviation) }.to_bytes(), b"UTC");
    }

    /// An unknown zone name returns the `i64::MIN` sentinel, telling the caller
    /// to fall back to its own (non-bridge) offset resolution.
    #[test]
    fn elephc_tz_offset_unknown_zone_is_sentinel() {
        let name = CString::new("Not/AZone").unwrap();
        let packed = unsafe { elephc_tz_offset(name.as_ptr(), 0) };
        assert_eq!(packed, i64::MIN);
    }

    /// A null name pointer also resolves to the sentinel (empty zone name is
    /// unknown), rather than panicking.
    #[test]
    fn elephc_tz_offset_null_name_is_sentinel() {
        let packed = unsafe { elephc_tz_offset(std::ptr::null(), 0) };
        assert_eq!(packed, i64::MIN);
    }

    /// A panic in a string export is contained and returned as the valid empty
    /// C-string sentinel rather than unwinding across the ABI boundary.
    #[test]
    fn string_export_panic_is_empty_sentinel() {
        let cell = Box::leak(Box::new(Mutex::new(CString::default())));
        let ptr = catch_string_export(cell, || panic!("injected tz string failure"));
        let value = unsafe { CStr::from_ptr(ptr) };
        assert_eq!(value.to_bytes(), b"");
    }

    /// A panic in the scalar offset export is contained and returned as the
    /// documented impossible-offset sentinel.
    #[test]
    fn scalar_export_panic_is_min_sentinel() {
        let value = catch_scalar_export(|| panic!("injected tz offset failure"), i64::MIN);
        assert_eq!(value, i64::MIN);
    }

    /// A panic while holding a result-buffer mutex poisons it, after which stash
    /// recovers the inner value and still publishes a valid replacement string.
    #[test]
    fn stash_recovers_poisoned_buffer() {
        let cell: &'static Mutex<CString> =
            Box::leak(Box::new(Mutex::new(CString::default())));
        let cell_for_panic = cell;
        let _ = std::thread::spawn(move || {
            let _guard = cell_for_panic.lock().unwrap();
            panic!("inject buffer poison");
        })
        .join();

        let ptr = stash(cell, "recovered".to_string());
        let value = unsafe { CStr::from_ptr(ptr) };
        assert_eq!(value.to_bytes(), b"recovered");
    }
}
