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

use crate::{
    abbreviations, format, timelib_ffi, timezone_identifier_valid, zone_location,
    zone_transitions,
};

/// Reads one borrowed UTF-8 string from elephc's pointer-and-length string ABI.
///
/// Invalid pointers remain a caller contract violation; invalid UTF-8 is rejected
/// as an empty value so timelib reports the same parse failure as an empty input.
unsafe fn sized_string<'a>(ptr: *const u8, len: i64) -> Cow<'a, str> {
    if ptr.is_null() || len <= 0 {
        return Cow::Borrowed("");
    }
    String::from_utf8_lossy(std::slice::from_raw_parts(ptr, len as usize))
}

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

/// Returns the process-wide buffer cell for raw timelib parse results.
fn parse_cell() -> &'static Mutex<CString> {
    static CELL: OnceLock<Mutex<CString>> = OnceLock::new();
    CELL.get_or_init(|| Mutex::new(CString::default()))
}

/// Returns the process-wide buffer cell for formatted date strings.
fn format_cell() -> &'static Mutex<CString> {
    static CELL: OnceLock<Mutex<CString>> = OnceLock::new();
    CELL.get_or_init(|| Mutex::new(CString::default()))
}

/// C ABI: formats a signed Unix timestamp through vendored timelib.
///
/// `localtime` selects the supplied timezone (`1`, PHP `date()`) or UTC (`0`,
/// PHP `gmdate()`). The returned NUL-terminated pointer remains valid until the
/// next call to this formatter.
///
/// # Safety
/// Both pointer/length pairs must designate readable byte slices.
#[no_mangle]
pub unsafe extern "C" fn elephc_tz_format(
    timestamp: i64,
    microsecond: i64,
    format_ptr: *const u8,
    format_len: i64,
    timezone_ptr: *const u8,
    timezone_len: i64,
    localtime: i64,
) -> *const c_char {
    let format_value = sized_string(format_ptr, format_len);
    let timezone = sized_string(timezone_ptr, timezone_len);
    let output = format::format_timestamp(
        timestamp,
        microsecond,
        &timezone,
        &format_value,
        localtime != 0,
    )
    .unwrap_or_default();
    stash(format_cell(), output)
}

/// C ABI: formats a timestamp with separately retained civil date fields.
///
/// # Safety
/// Both pointer/length pairs must designate readable byte slices.
#[no_mangle]
pub unsafe extern "C" fn elephc_tz_format_civil(
    timestamp: i64,
    microsecond: i64,
    format_ptr: *const u8,
    format_len: i64,
    payload_ptr: *const u8,
    payload_len: i64,
) -> *const c_char {
    let format_value = sized_string(format_ptr, format_len);
    let payload = sized_string(payload_ptr, payload_len);
    let mut fields = payload.split('\t');
    let Some(timezone) = fields.next() else {
        return stash(format_cell(), String::new());
    };
    let Some(year) = fields.next().and_then(|field| field.parse::<i64>().ok()) else {
        return stash(format_cell(), String::new());
    };
    let Some(month) = fields.next().and_then(|field| field.parse::<i64>().ok()) else {
        return stash(format_cell(), String::new());
    };
    let Some(day) = fields.next().and_then(|field| field.parse::<i64>().ok()) else {
        return stash(format_cell(), String::new());
    };
    if fields.next().is_some() {
        return stash(format_cell(), String::new());
    }
    let output = format::format_civil_timestamp(
        timestamp,
        microsecond,
        timezone,
        &format_value,
        true,
        year,
        month,
        day,
    )
    .unwrap_or_default();
    stash(format_cell(), output)
}

/// C ABI: computes a local PHP `mktime()` timestamp through vendored timelib.
///
/// The active timezone is read from `TZ`, which Elephc's timezone runtime keeps
/// synchronized with `date_default_timezone_set()` before this call.
#[no_mangle]
pub extern "C" fn elephc_tz_mktime(
    hour: i64,
    minute: i64,
    second: i64,
    month: i64,
    day: i64,
    year: i64,
) -> i64 {
    let timezone = std::env::var("TZ").unwrap_or_else(|_| "UTC".to_string());
    timelib_ffi::mktime_timestamp(hour, minute, second, month, day, year, &timezone)
        .unwrap_or(-1)
}

/// C ABI: computes a UTC PHP `gmmktime()` timestamp through vendored timelib.
#[no_mangle]
pub extern "C" fn elephc_tz_gmmktime(
    hour: i64,
    minute: i64,
    second: i64,
    month: i64,
    day: i64,
    year: i64,
) -> i64 {
    timelib_ffi::mktime_timestamp(hour, minute, second, month, day, year, "UTC")
        .unwrap_or(-1)
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

/// C ABI: reports whether a pointer-and-length string is an accepted php-src
/// timezone identifier for `date_default_timezone_set()`.
///
/// # Safety
/// `name_ptr` and `name_len` must designate a readable byte slice.
#[no_mangle]
pub unsafe extern "C" fn elephc_tz_timezone_valid(
    name_ptr: *const u8,
    name_len: i64,
) -> i64 {
    let name = sized_string(name_ptr, name_len);
    i64::from(timezone_identifier_valid(&name))
}

/// C ABI: returns php-src's `date_parse()` field/diagnostic structure serialized
/// as tab-separated records for the Elephc-PHP marshalling helper.
///
/// # Safety
/// `input_ptr` and `input_len` must designate a readable byte slice.
#[no_mangle]
pub unsafe extern "C" fn elephc_tz_date_parse(
    input_ptr: *const u8,
    input_len: i64,
) -> *const c_char {
    let input = sized_string(input_ptr, input_len);
    stash(parse_cell(), timelib_ffi::parse_serialized(None, &input))
}

/// C ABI: returns php-src's `date_parse_from_format()` field/diagnostic structure
/// serialized as tab-separated records for the Elephc-PHP marshalling helper.
///
/// # Safety
/// Both pointer/length pairs must designate readable byte slices.
#[no_mangle]
pub unsafe extern "C" fn elephc_tz_date_parse_from_format(
    format_ptr: *const u8,
    format_len: i64,
    input_ptr: *const u8,
    input_len: i64,
) -> *const c_char {
    let format = sized_string(format_ptr, format_len);
    let input = sized_string(input_ptr, input_len);
    stash(
        parse_cell(),
        timelib_ffi::parse_serialized(Some(&format), &input),
    )
}

/// C ABI: parses and normalizes `DateTime::createFromFormat()` through timelib,
/// returning the calculated timestamp, timezone representation, and complete
/// diagnostics in the shared serialized record format.
///
/// # Safety
/// All pointer/length pairs must designate readable byte slices.
#[no_mangle]
pub unsafe extern "C" fn elephc_tz_create_from_format(
    format_ptr: *const u8,
    format_len: i64,
    input_ptr: *const u8,
    input_len: i64,
    base_timestamp: i64,
    timezone_ptr: *const u8,
    timezone_len: i64,
) -> *const c_char {
    let format = sized_string(format_ptr, format_len);
    let input = sized_string(input_ptr, input_len);
    let timezone = sized_string(timezone_ptr, timezone_len);
    stash(
        parse_cell(),
        timelib_ffi::create_from_format_serialized(
            &format,
            &input,
            base_timestamp,
            &timezone,
        ),
    )
}

/// C ABI: parses a DateInterval duration or free-form string through php-src's
/// timelib and returns the complete relative-time record.
///
/// `relative` selects ISO duration parsing (`0`), `createFromDateString()`
/// parsing that rejects absolute fields (`1`), or serialization restoration
/// that accepts absolute fields while retaining only their relative part (`2`).
///
/// # Safety
/// The pointer/length pair must designate a readable byte slice.
#[no_mangle]
pub unsafe extern "C" fn elephc_tz_interval_parse(
    input_ptr: *const u8,
    input_len: i64,
    relative: i64,
) -> *const c_char {
    let input = sized_string(input_ptr, input_len);
    let serialized = if relative == 2 {
        timelib_ffi::interval_restore_parse_serialized(&input)
    } else {
        timelib_ffi::interval_parse_serialized(&input, relative != 0)
    };
    stash(parse_cell(), serialized)
}

/// C ABI: parses DatePeriod's ISO interval grammar and returns its constituent
/// start/end/period/recurrence fields.
///
/// # Safety
/// The pointer/length pair must designate a readable byte slice.
#[no_mangle]
pub unsafe extern "C" fn elephc_tz_period_parse(
    input_ptr: *const u8,
    input_len: i64,
) -> *const c_char {
    let input = sized_string(input_ptr, input_len);
    stash(parse_cell(), timelib_ffi::period_parse_serialized(&input))
}

/// C ABI: applies one serialized DateInterval to a zoned timestamp through
/// timelib's civil/wall add or subtract implementation.
///
/// # Safety
/// Both pointer/length pairs must designate readable byte slices.
#[no_mangle]
pub unsafe extern "C" fn elephc_tz_apply_interval(
    timestamp: i64,
    microsecond: i64,
    timezone_ptr: *const u8,
    timezone_len: i64,
    payload_ptr: *const u8,
    payload_len: i64,
    subtract: i64,
) -> *const c_char {
    let timezone = sized_string(timezone_ptr, timezone_len);
    let payload = sized_string(payload_ptr, payload_len);
    let serialized = timelib_ffi::apply_interval_serialized(
        timestamp,
        microsecond,
        &timezone,
        &payload,
        subtract != 0,
    )
    .unwrap_or_default();
    stash(parse_cell(), serialized)
}

/// C ABI: applies php-src's `DateTime::modify()` algorithm to a zoned instant.
///
/// # Safety
/// Both pointer/length pairs must designate readable byte slices.
#[no_mangle]
pub unsafe extern "C" fn elephc_tz_modify(
    timestamp: i64,
    microsecond: i64,
    timezone_ptr: *const u8,
    timezone_len: i64,
    modifier_ptr: *const u8,
    modifier_len: i64,
) -> *const c_char {
    let timezone = sized_string(timezone_ptr, timezone_len);
    let modifier = sized_string(modifier_ptr, modifier_len);
    let serialized = timelib_ffi::modify_serialized(
        timestamp,
        microsecond,
        &timezone,
        &modifier,
    )
    .unwrap_or_default();
    stash(parse_cell(), serialized)
}

/// C ABI: replaces the civil date or time fields of one zoned instant.
///
/// # Safety
/// Both pointer/length pairs must designate readable byte slices.
#[no_mangle]
pub unsafe extern "C" fn elephc_tz_set_civil(
    timestamp: i64,
    microsecond: i64,
    timezone_ptr: *const u8,
    timezone_len: i64,
    payload_ptr: *const u8,
    payload_len: i64,
) -> *const c_char {
    let timezone = sized_string(timezone_ptr, timezone_len);
    let payload = sized_string(payload_ptr, payload_len);
    let serialized =
        timelib_ffi::set_civil_serialized(timestamp, microsecond, &timezone, &payload)
            .unwrap_or_default();
    stash(parse_cell(), serialized)
}

/// C ABI: applies `DateTime::setISODate()` and returns timestamp plus civil date fields.
///
/// # Safety
/// The timezone pointer/length pair must designate a readable byte slice.
#[no_mangle]
pub unsafe extern "C" fn elephc_tz_set_iso_date(
    timestamp: i64,
    microsecond: i64,
    timezone_ptr: *const u8,
    timezone_len: i64,
    year: i64,
    week: i64,
    day: i64,
) -> *const c_char {
    let timezone = sized_string(timezone_ptr, timezone_len);
    let serialized = timelib_ffi::set_iso_date_serialized(
        timestamp,
        microsecond,
        &timezone,
        year,
        week,
        day,
    )
    .unwrap_or_default();
    stash(parse_cell(), serialized)
}

/// C ABI: computes php-src's zoned `DateTimeInterface::diff()` record.
///
/// # Safety
/// Both timezone pointer/length pairs must designate readable byte slices.
#[no_mangle]
pub unsafe extern "C" fn elephc_tz_diff(
    left_timestamp: i64,
    left_microsecond: i64,
    left_timezone_ptr: *const u8,
    left_timezone_len: i64,
    right_timestamp: i64,
    right_microsecond: i64,
    right_timezone_ptr: *const u8,
    right_timezone_len: i64,
) -> *const c_char {
    let left_timezone = sized_string(left_timezone_ptr, left_timezone_len);
    let right_timezone = sized_string(right_timezone_ptr, right_timezone_len);
    let serialized = timelib_ffi::diff_serialized(
        left_timestamp,
        left_microsecond,
        &left_timezone,
        right_timestamp,
        right_microsecond,
        &right_timezone,
    )
    .unwrap_or_default();
    stash(parse_cell(), serialized)
}

/// C ABI: parses a PHP free-form datetime through timelib.
///
/// Returns the Unix timestamp and writes `1` to `success` on success. On failure,
/// returns the legacy `i64::MIN` value and writes `0`; the separate flag keeps a
/// real php-src timestamp of `i64::MIN` distinguishable from parse failure.
/// `has_base` distinguishes an omitted base timestamp from the valid timestamp zero.
///
/// # Safety
/// Both pointer/length pairs must designate readable byte slices for the duration
/// of this call, and `success` must be null or point to one writable `i64`.
#[no_mangle]
pub unsafe extern "C" fn elephc_tz_strtotime(
    input_ptr: *const u8,
    input_len: i64,
    base_timestamp: i64,
    has_base: i64,
    timezone_ptr: *const u8,
    timezone_len: i64,
    success: *mut i64,
) -> i64 {
    let input = sized_string(input_ptr, input_len);
    let timezone_name = sized_string(timezone_ptr, timezone_len);
    let timestamp = timelib_ffi::strtotime_timestamp(
        &input,
        (has_base != 0).then_some(base_timestamp),
        &timezone_name,
    );
    if !success.is_null() {
        *success = i64::from(timestamp.is_some());
    }
    timestamp.unwrap_or(i64::MIN)
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

    /// Free-form parsing delegates to timelib for grammar PHP accepts beyond the
    /// former handwritten runtime parser.
    #[test]
    fn parses_free_form_dates_with_timelib() {
        let input = b"2024/06/15";
        let timezone = b"UTC";
        let mut success = 0;
        let timestamp = unsafe {
            elephc_tz_strtotime(
                input.as_ptr(),
                input.len() as i64,
                0,
                1,
                timezone.as_ptr(),
                timezone.len() as i64,
                &mut success,
            )
        };
        assert_eq!(success, 1);
        assert_eq!(timestamp, 1_718_409_600);
    }

    /// A successful `PHP_INT_MIN` timestamp remains distinct from parse failure.
    #[test]
    fn preserves_minimum_timestamp_success() {
        let input = b"@-9223372036854775808";
        let timezone = b"UTC";
        let mut success = 0;
        let timestamp = unsafe {
            elephc_tz_strtotime(
                input.as_ptr(),
                input.len() as i64,
                0,
                0,
                timezone.as_ptr(),
                timezone.len() as i64,
                &mut success,
            )
        };
        assert_eq!(success, 1);
        assert_eq!(timestamp, i64::MIN);
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
