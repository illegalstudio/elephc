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
//! - The returned pointer is owned by a per-thread `RefCell<CString>` and stays
//!   valid until the next call to the same function ON THAT THREAD. The compiled
//!   PHP program is single-threaded and copies the bytes immediately, mirroring
//!   the PDO bridge. The cells are per-thread rather than process-global for a
//!   lifetime reason: stashing DROPS the `CString` the cell held, so a global cell
//!   would let one thread free the bytes another thread was just handed a pointer
//!   into (`elephc-pdo` shipped exactly that bug — see
//!   `sqlstate_buffers_are_isolated_between_threads`).
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
use std::cell::RefCell;
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::thread::LocalKey;

use crate::{
    abbreviations, is_known_timezone_identifier, zone_location, zone_offset_at, zone_transitions,
};

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

/// Moves `s` into the calling thread's `cell` and returns a pointer to its
/// NUL-terminated bytes. The pointer is valid until the next call that stashes
/// into the same cell on that thread. A `String` carrying an interior NUL (never
/// produced here) degrades to empty.
///
/// The assignment drops the previous `CString`, which is why the cell is
/// per-thread: a shared cell would free those bytes under any other thread still
/// holding the pointer it was handed.
fn stash(cell: &'static LocalKey<RefCell<CString>>, s: String) -> *const c_char {
    let value = CString::new(s).unwrap_or_default();
    cell.with(|slot| {
        let mut slot = slot.borrow_mut();
        *slot = value;
        slot.as_ptr()
    })
}

/// Runs one string-returning C export behind an unwind barrier and stores its
/// result in the export's stable buffer. A panic becomes a valid empty C string,
/// matching the existing false-zone/error sentinel instead of crossing the ABI.
fn catch_string_export(
    cell: &'static LocalKey<RefCell<CString>>,
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

/// Returns the calling thread's buffer cell for transition results.
fn transitions_cell() -> &'static LocalKey<RefCell<CString>> {
    thread_local! {
        static CELL: RefCell<CString> = RefCell::new(CString::default());
    }
    &CELL
}

/// Returns the calling thread's buffer cell for location results.
fn location_cell() -> &'static LocalKey<RefCell<CString>> {
    thread_local! {
        static CELL: RefCell<CString> = RefCell::new(CString::default());
    }
    &CELL
}

/// Returns the calling thread's buffer cell for the abbreviation table.
fn abbreviations_cell() -> &'static LocalKey<RefCell<CString>> {
    thread_local! {
        static CELL: RefCell<CString> = RefCell::new(CString::default());
    }
    &CELL
}

/// Returns the calling thread's buffer cell for one transition abbreviation.
///
/// As with the other string exports, replacing this `CString` drops the prior
/// allocation. Keeping it thread-local prevents one request thread from
/// invalidating a pointer another thread has not copied yet.
fn abbreviation_cell() -> &'static LocalKey<RefCell<CString>> {
    thread_local! {
        static CELL: RefCell<CString> = RefCell::new(CString::default());
    }
    &CELL
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

/// C ABI: validates an exact, length-delimited PHP timezone identifier against
/// the baked PHP/timelib namespace. Returning an integer keeps the entry point
/// easy to call from the compiler runtime on every target.
///
/// The length is authoritative: embedded NUL bytes are not silently truncated
/// into a different accepted identifier before `date_default_timezone_set()`
/// commits its process-wide timezone state.
///
/// # Safety
/// `name` must point to `len` readable bytes, or `len` must be zero when the
/// pointer is null.
#[no_mangle]
pub unsafe extern "C" fn elephc_tz_is_valid(name: *const c_char, len: usize) -> c_int {
    let valid = catch_unwind(AssertUnwindSafe(|| {
        if name.is_null() || len == 0 {
            return false;
        }
        let bytes = std::slice::from_raw_parts(name.cast::<u8>(), len);
        std::str::from_utf8(bytes)
            .ok()
            .filter(|identifier| !identifier.as_bytes().contains(&0))
            .is_some_and(is_known_timezone_identifier)
    }))
    .unwrap_or(false);
    c_int::from(valid)
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
    //!   only pointer plumbing over them), plus one cross-thread test that covers
    //!   the pointer plumbing itself.

    use super::*;

    /// Each thread's stashed result stays alive until that thread has copied it
    /// out. Two threads asking for *different* zones at the same moment must each
    /// read back their own: with one process-wide cell, the second thread's
    /// `stash` drops the `CString` the first was just handed a pointer into, so
    /// that thread reads freed memory. (`elephc-pdo` shipped this exact bug and it
    /// reached CI as non-UTF-8 garbage where a SQLSTATE belonged.)
    #[test]
    fn location_buffers_are_isolated_between_threads() {
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
        let handles = ["Europe/Paris", "UTC"].map(|zone| {
            let barrier = std::sync::Arc::clone(&barrier);
            std::thread::spawn(move || {
                let expected = serialize_location(zone);
                assert!(!expected.is_empty(), "{zone} must serialize non-empty");
                let name = CString::new(zone).expect("zone name");

                // Both threads take their pointer before either reads it: that is
                // the window in which one process-wide cell frees the string the
                // other thread is still holding a pointer into.
                barrier.wait();
                let pointer = unsafe { elephc_tz_location(name.as_ptr()) };
                barrier.wait();
                let actual = unsafe { CStr::from_ptr(pointer) }
                    .to_string_lossy()
                    .into_owned();
                assert_eq!(
                    actual, expected,
                    "location buffer was overwritten by the other thread"
                );
            })
        });

        for handle in handles {
            handle.join().expect("tz location worker must not panic");
        }
    }

    /// The transition-abbreviation ABI has the same pointer lifetime contract
    /// as the bulk serialization exports. A second thread must not invalidate
    /// the first thread's pointer while it still copies its result.
    #[test]
    fn abbreviation_buffers_are_isolated_between_threads() {
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
        let handles = ["Europe/Paris", "UTC"].map(|zone| {
            let barrier = std::sync::Arc::clone(&barrier);
            std::thread::spawn(move || {
                let expected = zone_offset_at(zone, 1_705_320_000)
                    .expect("known test zone")
                    .2;
                let name = CString::new(zone).expect("zone name");

                barrier.wait();
                let pointer = unsafe { elephc_tz_abbreviation(name.as_ptr(), 1_705_320_000) };
                barrier.wait();
                let actual = unsafe { CStr::from_ptr(pointer) }
                    .to_string_lossy()
                    .into_owned();
                assert_eq!(actual, expected, "abbreviation buffer was overwritten");
            })
        });

        for handle in handles {
            handle.join().expect("tz abbreviation worker must not panic");
        }
    }

    /// The runtime validator receives a pointer plus length, so an embedded
    /// NUL must not be accepted as a truncated valid identifier.
    #[test]
    fn validation_uses_the_complete_length_delimited_identifier() {
        for name in ["UTC", "Europe/Paris", "America/New_York", "US/Eastern", "CET"] {
            let name = CString::new(name).expect("timezone identifier");
            assert_eq!(unsafe { elephc_tz_is_valid(name.as_ptr(), name.as_bytes().len()) }, 1);
        }
        let nul_padded = b"UTC\0not-a-zone";
        assert_eq!(
            unsafe { elephc_tz_is_valid(nul_padded.as_ptr().cast(), nul_padded.len()) },
            0
        );
        let invalid = CString::new("Europe/Nope").expect("invalid timezone identifier");
        assert_eq!(unsafe { elephc_tz_is_valid(invalid.as_ptr(), invalid.as_bytes().len()) }, 0);
    }

    /// The Windows offset entry point must preserve PHP aliases at the native
    /// ABI boundary, where the runtime passes a NUL-terminated zone name.
    #[test]
    fn offset_export_resolves_us_eastern_alias() {
        let alias = CString::new("US/Eastern").expect("timezone identifier");
        let canonical = CString::new("America/New_York").expect("timezone identifier");
        let alias_offset = unsafe { elephc_tz_offset(alias.as_ptr(), 1_719_835_200) };
        let canonical_offset = unsafe { elephc_tz_offset(canonical.as_ptr(), 1_719_835_200) };
        assert_eq!(alias_offset, canonical_offset);
        assert_eq!(alias_offset, -28_799);
    }

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
        let ptr = catch_string_export(transitions_cell(), || panic!("injected tz string failure"));
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

    /// Stashing a new value only affects this thread's pointer cell.
    #[test]
    fn stash_replaces_current_thread_buffer() {
        let ptr = stash(transitions_cell(), "recovered".to_string());
        let value = unsafe { CStr::from_ptr(ptr) };
        assert_eq!(value.to_bytes(), b"recovered");
    }
}
