//! Purpose:
//! Exposes the raw timelib parse result that php-src uses for `date_parse()` and
//! `date_parse_from_format()`, including every warning, error, relative field,
//! and parsed timezone discriminator.
//!
//! Called from:
//! - `crate::abi` to serialize timelib results across Elephc's C ABI.
//!
//! Key details:
//! - The layouts mirror the timelib sources vendored from the audited php-src
//!   commit; compile-time Rust and C size, alignment, and offset assertions
//!   guard the supported 64-bit target matrix against accidental ABI drift.
//! - Parsing does not fill unset fields. This is deliberate: php-src's
//!   `php_date_do_return_parsed_time()` exposes the raw parse structure.

use std::collections::HashMap;
use std::ffi::{CStr, CString, c_void};
use std::os::raw::{c_char, c_int, c_uint};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

const TIMELIB_UNSET: i64 = -9_999_999;
const TIMELIB_SPECIAL_WEEKDAY: c_uint = 1;
const TIMELIB_ZONETYPE_OFFSET: c_uint = 1;
const TIMELIB_ZONETYPE_ABBR: c_uint = 2;
const TIMELIB_ZONETYPE_ID: c_uint = 3;
const TIMELIB_OVERRIDE_TIME: c_int = 1;
const TIMELIB_NO_CLONE: c_int = 2;

#[repr(C)]
#[derive(Clone, Copy)]
struct TimelibRelativeSpecial {
    type_: c_uint,
    amount: i64,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct TimelibRelativeTime {
    y: i64,
    m: i64,
    d: i64,
    h: i64,
    i: i64,
    s: i64,
    us: i64,
    weekday: c_int,
    weekday_behavior: c_int,
    first_last_day_of: c_int,
    invert: c_int,
    days: i64,
    special: TimelibRelativeSpecial,
    have_weekday_relative: c_uint,
    have_special_relative: c_uint,
}

#[repr(C)]
#[allow(dead_code)]
struct TimelibTzCounts32 {
    ttisgmtcnt: u32,
    ttisstdcnt: u32,
    leapcnt: u32,
    timecnt: u32,
    typecnt: u32,
    charcnt: u32,
}

#[repr(C)]
#[allow(dead_code)]
struct TimelibTzCounts64 {
    ttisgmtcnt: u64,
    ttisstdcnt: u64,
    leapcnt: u64,
    timecnt: u64,
    typecnt: u64,
    charcnt: u64,
}

#[repr(C)]
#[allow(dead_code)]
struct TimelibLocationInfo {
    country_code: [c_char; 3],
    latitude: f64,
    longitude: f64,
    comments: *mut c_char,
}

#[repr(C)]
#[allow(dead_code)]
struct TimelibTimezoneInfo {
    name: *mut c_char,
    bit32: TimelibTzCounts32,
    bit64: TimelibTzCounts64,
    trans: *mut i64,
    trans_idx: *mut u8,
    type_info: *mut c_void,
    timezone_abbr: *mut c_char,
    leap_times: *mut c_void,
    bc: u8,
    location: TimelibLocationInfo,
    posix_string: *mut c_char,
    posix_info: *mut c_void,
}

#[repr(C)]
struct TimelibTime {
    y: i64,
    m: i64,
    d: i64,
    h: i64,
    i: i64,
    s: i64,
    us: i64,
    z: c_int,
    tz_abbr: *mut c_char,
    tz_info: *mut TimelibTimezoneInfo,
    dst: c_int,
    relative: TimelibRelativeTime,
    sse: i64,
    have_time: c_uint,
    have_date: c_uint,
    have_zone: c_uint,
    have_relative: c_uint,
    have_weeknr_day: c_uint,
    sse_uptodate: c_uint,
    tim_uptodate: c_uint,
    is_localtime: c_uint,
    zone_type: c_uint,
}

#[repr(C)]
struct TimelibErrorMessage {
    error_code: c_int,
    position: c_int,
    character: c_char,
    message: *mut c_char,
}

#[repr(C)]
struct TimelibErrorContainer {
    error_messages: *mut TimelibErrorMessage,
    warning_messages: *mut TimelibErrorMessage,
    error_count: c_int,
    warning_count: c_int,
}

#[repr(C)]
struct TimelibAbbreviationInfo {
    utc_offset: i64,
    abbreviation: *mut c_char,
    dst: c_int,
}

#[repr(C)]
struct TimelibTimezoneDb {
    _private: [u8; 0],
}

type TimelibTimezoneGetter = Option<
    unsafe extern "C" fn(
        *const c_char,
        *const TimelibTimezoneDb,
        *mut c_int,
    ) -> *mut TimelibTimezoneInfo,
>;

unsafe extern "C" {
    /// Parses PHP's free-form date grammar into a raw timelib time.
    fn timelib_strtotime(
        input: *const c_char,
        len: usize,
        errors: *mut *mut TimelibErrorContainer,
        tzdb: *const TimelibTimezoneDb,
        timezone_getter: TimelibTimezoneGetter,
    ) -> *mut TimelibTime;

    /// Parses one date according to PHP's `createFromFormat()` grammar.
    fn timelib_parse_from_format(
        format: *const c_char,
        input: *const c_char,
        len: usize,
        errors: *mut *mut TimelibErrorContainer,
        tzdb: *const TimelibTimezoneDb,
        timezone_getter: TimelibTimezoneGetter,
    ) -> *mut TimelibTime;

    /// Returns the immutable timezone database compiled with timelib.
    fn timelib_builtin_db() -> *const TimelibTimezoneDb;

    /// Parses one identifier from the compiled timezone database.
    fn timelib_parse_tzfile(
        name: *const c_char,
        tzdb: *const TimelibTimezoneDb,
        error_code: *mut c_int,
    ) -> *mut TimelibTimezoneInfo;

    /// Releases one raw parse result.
    fn timelib_time_dtor(time: *mut TimelibTime);

    /// Releases the warning/error container returned by a parser call.
    fn timelib_error_container_dtor(errors: *mut TimelibErrorContainer);

    /// Allocates one empty timelib time structure.
    fn timelib_time_ctor() -> *mut TimelibTime;

    /// Populates civil fields from a Unix timestamp using the attached zone.
    fn timelib_unixtime2local(time: *mut TimelibTime, timestamp: i64);

    /// Populates UTC civil fields from a Unix timestamp.
    fn timelib_unixtime2gmt(time: *mut TimelibTime, timestamp: i64);

    /// Fills unset parsed fields from the supplied current-time structure.
    fn timelib_fill_holes(parsed: *mut TimelibTime, now: *mut TimelibTime, options: c_int);

    /// Computes the Unix timestamp for populated civil fields.
    fn timelib_update_ts(time: *mut TimelibTime, timezone: *mut TimelibTimezoneInfo);

    /// Recomputes normalized civil fields from the calculated timestamp.
    fn timelib_update_from_sse(time: *mut TimelibTime);

    /// Attaches a fixed UTC offset to a time structure.
    fn timelib_set_timezone_from_offset(time: *mut TimelibTime, offset: i64);

    /// Attaches an abbreviation and its offset/DST data to a time structure.
    fn timelib_set_timezone_from_abbr(
        time: *mut TimelibTime,
        abbreviation: TimelibAbbreviationInfo,
    );

    /// Returns timelib's relative day offset for one ISO week date.
    fn timelib_daynr_from_weeknr(year: i64, week: i64, day: i64) -> i64;

    /// Parses php-src's ISO interval grammar into begin/end/relative parts.
    fn timelib_strtointerval(
        input: *const c_char,
        len: usize,
        begin: *mut *mut TimelibTime,
        end: *mut *mut TimelibTime,
        period: *mut *mut TimelibRelativeTime,
        recurrences: *mut c_int,
        errors: *mut *mut TimelibErrorContainer,
    );

    /// Releases one relative interval allocated by timelib.
    fn timelib_rel_time_dtor(relative: *mut TimelibRelativeTime);

    /// Computes the civil difference between two parsed ISO endpoints.
    fn timelib_diff(
        one: *mut TimelibTime,
        two: *mut TimelibTime,
    ) -> *mut TimelibRelativeTime;

    /// Applies a civil interval to one initialized time.
    fn timelib_add(
        time: *mut TimelibTime,
        interval: *mut TimelibRelativeTime,
    ) -> *mut TimelibTime;

    /// Applies a wall-clock interval to one initialized time.
    fn timelib_add_wall(
        time: *mut TimelibTime,
        interval: *mut TimelibRelativeTime,
    ) -> *mut TimelibTime;

    /// Subtracts a civil interval from one initialized time.
    fn timelib_sub(
        time: *mut TimelibTime,
        interval: *mut TimelibRelativeTime,
    ) -> *mut TimelibTime;

    /// Subtracts a wall-clock interval from one initialized time.
    fn timelib_sub_wall(
        time: *mut TimelibTime,
        interval: *mut TimelibRelativeTime,
    ) -> *mut TimelibTime;
}

/// Returns the process-wide cache used by timelib's timezone callback.
fn timezone_cache() -> &'static Mutex<HashMap<String, usize>> {
    static CACHE: OnceLock<Mutex<HashMap<String, usize>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Resolves a timezone identifier through php-src's database and retains the
/// immutable rules object for later parser calls.
unsafe extern "C" fn cached_timezone_getter(
    name: *const c_char,
    tzdb: *const TimelibTimezoneDb,
    error_code: *mut c_int,
) -> *mut TimelibTimezoneInfo {
    if name.is_null() {
        return std::ptr::null_mut();
    }
    let name = CStr::from_ptr(name).to_string_lossy().into_owned();
    let Ok(mut cache) = timezone_cache().lock() else {
        return std::ptr::null_mut();
    };
    if let Some(pointer) = cache.get(&name) {
        return *pointer as *mut TimelibTimezoneInfo;
    }
    let Ok(c_name) = CString::new(name.as_str()) else {
        return std::ptr::null_mut();
    };
    let pointer = timelib_parse_tzfile(c_name.as_ptr(), tzdb, error_code);
    if !pointer.is_null() {
        cache.insert(name, pointer as usize);
    }
    pointer
}

const _: () = {
    assert!(std::mem::align_of::<TimelibRelativeSpecial>() == 8);
    assert!(std::mem::size_of::<TimelibRelativeSpecial>() == 16);
    assert!(std::mem::offset_of!(TimelibRelativeSpecial, type_) == 0);
    assert!(std::mem::offset_of!(TimelibRelativeSpecial, amount) == 8);

    assert!(std::mem::align_of::<TimelibRelativeTime>() == 8);
    assert!(std::mem::size_of::<TimelibRelativeTime>() == 104);
    assert!(std::mem::offset_of!(TimelibRelativeTime, y) == 0);
    assert!(std::mem::offset_of!(TimelibRelativeTime, m) == 8);
    assert!(std::mem::offset_of!(TimelibRelativeTime, d) == 16);
    assert!(std::mem::offset_of!(TimelibRelativeTime, h) == 24);
    assert!(std::mem::offset_of!(TimelibRelativeTime, i) == 32);
    assert!(std::mem::offset_of!(TimelibRelativeTime, s) == 40);
    assert!(std::mem::offset_of!(TimelibRelativeTime, us) == 48);
    assert!(std::mem::offset_of!(TimelibRelativeTime, weekday) == 56);
    assert!(std::mem::offset_of!(TimelibRelativeTime, weekday_behavior) == 60);
    assert!(std::mem::offset_of!(TimelibRelativeTime, first_last_day_of) == 64);
    assert!(std::mem::offset_of!(TimelibRelativeTime, invert) == 68);
    assert!(std::mem::offset_of!(TimelibRelativeTime, days) == 72);
    assert!(std::mem::offset_of!(TimelibRelativeTime, special) == 80);
    assert!(std::mem::offset_of!(TimelibRelativeTime, have_weekday_relative) == 96);
    assert!(std::mem::offset_of!(TimelibRelativeTime, have_special_relative) == 100);

    assert!(std::mem::align_of::<TimelibTzCounts32>() == 4);
    assert!(std::mem::size_of::<TimelibTzCounts32>() == 24);
    assert!(std::mem::offset_of!(TimelibTzCounts32, ttisgmtcnt) == 0);
    assert!(std::mem::offset_of!(TimelibTzCounts32, ttisstdcnt) == 4);
    assert!(std::mem::offset_of!(TimelibTzCounts32, leapcnt) == 8);
    assert!(std::mem::offset_of!(TimelibTzCounts32, timecnt) == 12);
    assert!(std::mem::offset_of!(TimelibTzCounts32, typecnt) == 16);
    assert!(std::mem::offset_of!(TimelibTzCounts32, charcnt) == 20);

    assert!(std::mem::align_of::<TimelibTzCounts64>() == 8);
    assert!(std::mem::size_of::<TimelibTzCounts64>() == 48);
    assert!(std::mem::offset_of!(TimelibTzCounts64, ttisgmtcnt) == 0);
    assert!(std::mem::offset_of!(TimelibTzCounts64, ttisstdcnt) == 8);
    assert!(std::mem::offset_of!(TimelibTzCounts64, leapcnt) == 16);
    assert!(std::mem::offset_of!(TimelibTzCounts64, timecnt) == 24);
    assert!(std::mem::offset_of!(TimelibTzCounts64, typecnt) == 32);
    assert!(std::mem::offset_of!(TimelibTzCounts64, charcnt) == 40);

    assert!(std::mem::align_of::<TimelibLocationInfo>() == 8);
    assert!(std::mem::size_of::<TimelibLocationInfo>() == 32);
    assert!(std::mem::offset_of!(TimelibLocationInfo, country_code) == 0);
    assert!(std::mem::offset_of!(TimelibLocationInfo, latitude) == 8);
    assert!(std::mem::offset_of!(TimelibLocationInfo, longitude) == 16);
    assert!(std::mem::offset_of!(TimelibLocationInfo, comments) == 24);

    assert!(std::mem::align_of::<TimelibTimezoneInfo>() == 8);
    assert!(std::mem::size_of::<TimelibTimezoneInfo>() == 176);
    assert!(std::mem::offset_of!(TimelibTimezoneInfo, name) == 0);
    assert!(std::mem::offset_of!(TimelibTimezoneInfo, bit32) == 8);
    assert!(std::mem::offset_of!(TimelibTimezoneInfo, bit64) == 32);
    assert!(std::mem::offset_of!(TimelibTimezoneInfo, trans) == 80);
    assert!(std::mem::offset_of!(TimelibTimezoneInfo, trans_idx) == 88);
    assert!(std::mem::offset_of!(TimelibTimezoneInfo, type_info) == 96);
    assert!(std::mem::offset_of!(TimelibTimezoneInfo, timezone_abbr) == 104);
    assert!(std::mem::offset_of!(TimelibTimezoneInfo, leap_times) == 112);
    assert!(std::mem::offset_of!(TimelibTimezoneInfo, bc) == 120);
    assert!(std::mem::offset_of!(TimelibTimezoneInfo, location) == 128);
    assert!(std::mem::offset_of!(TimelibTimezoneInfo, posix_string) == 160);
    assert!(std::mem::offset_of!(TimelibTimezoneInfo, posix_info) == 168);

    assert!(std::mem::align_of::<TimelibTime>() == 8);
    assert!(std::mem::size_of::<TimelibTime>() == 240);
    assert!(std::mem::offset_of!(TimelibTime, y) == 0);
    assert!(std::mem::offset_of!(TimelibTime, m) == 8);
    assert!(std::mem::offset_of!(TimelibTime, d) == 16);
    assert!(std::mem::offset_of!(TimelibTime, h) == 24);
    assert!(std::mem::offset_of!(TimelibTime, i) == 32);
    assert!(std::mem::offset_of!(TimelibTime, s) == 40);
    assert!(std::mem::offset_of!(TimelibTime, us) == 48);
    assert!(std::mem::offset_of!(TimelibTime, z) == 56);
    assert!(std::mem::offset_of!(TimelibTime, tz_abbr) == 64);
    assert!(std::mem::offset_of!(TimelibTime, tz_info) == 72);
    assert!(std::mem::offset_of!(TimelibTime, dst) == 80);
    assert!(std::mem::offset_of!(TimelibTime, relative) == 88);
    assert!(std::mem::offset_of!(TimelibTime, sse) == 192);
    assert!(std::mem::offset_of!(TimelibTime, have_time) == 200);
    assert!(std::mem::offset_of!(TimelibTime, have_date) == 204);
    assert!(std::mem::offset_of!(TimelibTime, have_zone) == 208);
    assert!(std::mem::offset_of!(TimelibTime, have_relative) == 212);
    assert!(std::mem::offset_of!(TimelibTime, have_weeknr_day) == 216);
    assert!(std::mem::offset_of!(TimelibTime, sse_uptodate) == 220);
    assert!(std::mem::offset_of!(TimelibTime, tim_uptodate) == 224);
    assert!(std::mem::offset_of!(TimelibTime, is_localtime) == 228);
    assert!(std::mem::offset_of!(TimelibTime, zone_type) == 232);

    assert!(std::mem::align_of::<TimelibErrorMessage>() == 8);
    assert!(std::mem::size_of::<TimelibErrorMessage>() == 24);
    assert!(std::mem::offset_of!(TimelibErrorMessage, error_code) == 0);
    assert!(std::mem::offset_of!(TimelibErrorMessage, position) == 4);
    assert!(std::mem::offset_of!(TimelibErrorMessage, character) == 8);
    assert!(std::mem::offset_of!(TimelibErrorMessage, message) == 16);

    assert!(std::mem::align_of::<TimelibErrorContainer>() == 8);
    assert!(std::mem::size_of::<TimelibErrorContainer>() == 24);
    assert!(std::mem::offset_of!(TimelibErrorContainer, error_messages) == 0);
    assert!(std::mem::offset_of!(TimelibErrorContainer, warning_messages) == 8);
    assert!(std::mem::offset_of!(TimelibErrorContainer, error_count) == 16);
    assert!(std::mem::offset_of!(TimelibErrorContainer, warning_count) == 20);

    assert!(std::mem::align_of::<TimelibAbbreviationInfo>() == 8);
    assert!(std::mem::size_of::<TimelibAbbreviationInfo>() == 24);
    assert!(std::mem::offset_of!(TimelibAbbreviationInfo, utc_offset) == 0);
    assert!(std::mem::offset_of!(TimelibAbbreviationInfo, abbreviation) == 8);
    assert!(std::mem::offset_of!(TimelibAbbreviationInfo, dst) == 16);
};

/// Converts a nullable C string into a lossless-enough owned Rust string.
///
/// Timelib messages, abbreviations, and timezone identifiers are UTF-8/ASCII.
unsafe fn owned_c_string(value: *const c_char) -> String {
    if value.is_null() {
        String::new()
    } else {
        CStr::from_ptr(value).to_string_lossy().into_owned()
    }
}

/// Appends warning/error records as `<kind>\t<position>\t<message>` lines.
unsafe fn append_diagnostics(
    output: &mut String,
    kind: char,
    messages: *const TimelibErrorMessage,
    count: c_int,
) {
    if messages.is_null() || count <= 0 {
        return;
    }
    for index in 0..count as usize {
        let message = &*messages.add(index);
        output.push('\n');
        output.push(kind);
        output.push('\t');
        output.push_str(&message.position.to_string());
        output.push('\t');
        output.push_str(&owned_c_string(message.message));
    }
}

/// Serializes the exact fields php-src exposes from one raw timelib parse.
unsafe fn serialize_parsed(
    parsed: *mut TimelibTime,
    errors: *mut TimelibErrorContainer,
) -> String {
    if parsed.is_null() || errors.is_null() {
        return format!(
            "P\t{0}\t{0}\t{0}\t{0}\t{0}\t{0}\t{0}\t0\t0\t{0}\t0\t\t\t0\t1\t0\t{0}\nE\t0\tThe timezone could not be found in the database",
            TIMELIB_UNSET
        );
    }

    let time = &*parsed;
    let diagnostics = &*errors;
    let abbreviation = owned_c_string(time.tz_abbr);
    let timezone_id = if time.tz_info.is_null() {
        String::new()
    } else {
        owned_c_string((*time.tz_info).name)
    };
    let mut output = format!(
        "P\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
        time.y,
        time.m,
        time.d,
        time.h,
        time.i,
        time.s,
        time.us,
        time.is_localtime,
        time.zone_type,
        time.z,
        time.dst,
        abbreviation,
        timezone_id,
        diagnostics.warning_count,
        diagnostics.error_count,
        time.have_relative,
        time.sse,
    );
    append_diagnostics(
        &mut output,
        'W',
        diagnostics.warning_messages,
        diagnostics.warning_count,
    );
    append_diagnostics(
        &mut output,
        'E',
        diagnostics.error_messages,
        diagnostics.error_count,
    );

    if time.have_relative != 0 {
        let relative = &time.relative;
        output.push_str(&format!(
            "\nR\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            relative.y,
            relative.m,
            relative.d,
            relative.h,
            relative.i,
            relative.s,
            if relative.have_weekday_relative != 0 {
                relative.weekday as i64
            } else {
                TIMELIB_UNSET
            },
            if relative.have_special_relative != 0
                && relative.special.type_ == TIMELIB_SPECIAL_WEEKDAY
            {
                relative.special.amount
            } else {
                TIMELIB_UNSET
            },
            relative.first_last_day_of,
        ));
    }
    output
}

/// Parses a free-form or formatted string and serializes php-src's raw result.
pub fn parse_serialized(format: Option<&str>, input: &str) -> String {
    let Ok(input) = CString::new(input) else {
        return format!(
            "P\t{0}\t{0}\t{0}\t{0}\t{0}\t{0}\t{0}\t0\t0\t{0}\t0\t\t\t0\t1\t0\t{0}\nE\t0\tUnexpected data found.",
            TIMELIB_UNSET
        );
    };
    let format = format.and_then(|value| CString::new(value).ok());
    unsafe {
        let mut errors = std::ptr::null_mut();
        let parsed = match format.as_ref() {
            Some(format) => timelib_parse_from_format(
                format.as_ptr(),
                input.as_ptr(),
                input.as_bytes().len(),
                &mut errors,
                timelib_builtin_db(),
                Some(cached_timezone_getter),
            ),
            None => timelib_strtotime(
                input.as_ptr(),
                input.as_bytes().len(),
                &mut errors,
                timelib_builtin_db(),
                Some(cached_timezone_getter),
            ),
        };
        let serialized = serialize_parsed(parsed, errors);
        if !parsed.is_null() {
            timelib_time_dtor(parsed);
        }
        if !errors.is_null() {
            timelib_error_container_dtor(errors);
        }
        serialized
    }
}

/// Fully normalized timelib fields needed by php-src's date formatter.
pub(crate) struct TimestampParts {
    pub(crate) year: i64,
    pub(crate) month: i64,
    pub(crate) day: i64,
    pub(crate) hour: i64,
    pub(crate) minute: i64,
    pub(crate) second: i64,
    pub(crate) microsecond: i64,
    pub(crate) timestamp: i64,
    pub(crate) offset: i64,
    pub(crate) dst: i64,
    pub(crate) zone_type: u32,
    pub(crate) abbreviation: String,
    pub(crate) timezone_id: String,
    pub(crate) localtime: bool,
}

/// Converts one Unix timestamp through vendored timelib for php-src-compatible formatting.
pub(crate) fn timestamp_parts(
    timestamp: i64,
    microsecond: i64,
    timezone_name: &str,
    localtime: bool,
) -> Option<TimestampParts> {
    let timezone_name = CString::new(timezone_name).ok()?;
    unsafe {
        let time = timelib_time_ctor();
        if time.is_null() {
            return None;
        }
        if localtime {
            attach_timezone(time, &timezone_name);
            if (*time).is_localtime == 0 {
                timelib_time_dtor(time);
                return None;
            }
            timelib_unixtime2local(time, timestamp);
        } else {
            timelib_unixtime2gmt(time, timestamp);
        }
        (*time).us = microsecond;
        let timezone_id = if (*time).tz_info.is_null() {
            String::new()
        } else {
            owned_c_string((*(*time).tz_info).name)
        };
        let result = TimestampParts {
            year: (*time).y,
            month: (*time).m,
            day: (*time).d,
            hour: (*time).h,
            minute: (*time).i,
            second: (*time).s,
            microsecond: (*time).us,
            timestamp: (*time).sse,
            offset: (*time).z as i64,
            dst: (*time).dst as i64,
            zone_type: (*time).zone_type,
            abbreviation: owned_c_string((*time).tz_abbr),
            timezone_id,
            localtime,
        };
        timelib_time_dtor(time);
        Some(result)
    }
}

/// Converts PHP `mktime()` civil components through vendored timelib.
///
/// The full year remains an `i64`; unlike libc's `struct tm`, this preserves
/// php-src's large-year range and applies historical timezone offsets before
/// calculating the timestamp.
pub fn mktime_timestamp(
    hour: i64,
    minute: i64,
    second: i64,
    month: i64,
    day: i64,
    year: i64,
    timezone_name: &str,
) -> Option<i64> {
    let year = match year {
        0..=69 => year + 2_000,
        70..=100 => year + 1_900,
        _ => year,
    };
    let timezone_name = CString::new(timezone_name).ok()?;
    unsafe {
        let time = timelib_time_ctor();
        if time.is_null() {
            return None;
        }
        let timezone = attach_timezone(time, &timezone_name);
        if (*time).is_localtime == 0 {
            timelib_time_dtor(time);
            return None;
        }
        (*time).y = year;
        (*time).m = month;
        (*time).d = day;
        (*time).h = hour;
        (*time).i = minute;
        (*time).s = second;
        (*time).us = 0;
        (*time).have_date = 1;
        (*time).have_time = 1;
        (*time).sse_uptodate = 0;
        (*time).tim_uptodate = 1;
        timelib_update_ts(time, timezone);
        let timestamp = (*time).sse;
        timelib_time_dtor(time);
        Some(timestamp)
    }
}

/// Parses the POSIX `UTC-2[:30[:45]]` form used internally by Elephc's libc
/// timezone adapter and returns the corresponding east-of-UTC offset.
fn parse_posix_utc_offset(name: &str) -> Option<i64> {
    let rest = name.strip_prefix("UTC")?;
    let (sign, fields) = match rest.as_bytes().first().copied()? {
        b'+' => (-1_i64, &rest[1..]),
        b'-' => (1_i64, &rest[1..]),
        _ => return None,
    };
    let mut parts = fields.split(':');
    let hours = parts.next()?.parse::<i64>().ok()?;
    let minutes = parts.next().map_or(Some(0), |value| value.parse().ok())?;
    let seconds = parts.next().map_or(Some(0), |value| value.parse().ok())?;
    if parts.next().is_some()
        || hours > 99
        || minutes > 59
        || seconds > 59
    {
        return None;
    }
    Some(sign * (hours * 3_600 + minutes * 60 + seconds))
}

/// Parses a timezone name through timelib and attaches the resulting zone
/// representation to `time`. Returns the IANA rules pointer for ID zones.
unsafe fn attach_timezone(
    time: *mut TimelibTime,
    timezone_name: &CString,
) -> *mut TimelibTimezoneInfo {
    if let Ok(name) = timezone_name.to_str() {
        if let Some(offset) = parse_posix_utc_offset(name) {
            timelib_set_timezone_from_offset(time, offset);
            (*time).is_localtime = 1;
            return std::ptr::null_mut();
        }
    }
    let mut timezone_error = 0;
    let named_timezone = cached_timezone_getter(
        timezone_name.as_ptr(),
        timelib_builtin_db(),
        &mut timezone_error,
    );
    if !named_timezone.is_null() {
        (*time).zone_type = TIMELIB_ZONETYPE_ID;
        (*time).tz_info = named_timezone;
        (*time).is_localtime = 1;
        return named_timezone;
    }
    let mut errors = std::ptr::null_mut();
    let parsed_zone = timelib_strtotime(
        timezone_name.as_ptr(),
        timezone_name.as_bytes().len(),
        &mut errors,
        timelib_builtin_db(),
        Some(cached_timezone_getter),
    );
    let valid = !parsed_zone.is_null()
        && (errors.is_null() || (*errors).error_count == 0)
        && (*parsed_zone).is_localtime != 0;
    let mut timezone = std::ptr::null_mut();
    if valid {
        match (*parsed_zone).zone_type {
            TIMELIB_ZONETYPE_OFFSET => {
                timelib_set_timezone_from_offset(time, (*parsed_zone).z as i64);
                (*time).is_localtime = 1;
            }
            TIMELIB_ZONETYPE_ABBR => {
                timelib_set_timezone_from_abbr(
                    time,
                    TimelibAbbreviationInfo {
                        utc_offset: (*parsed_zone).z as i64,
                        abbreviation: (*parsed_zone).tz_abbr,
                        dst: (*parsed_zone).dst,
                    },
                );
                (*time).is_localtime = 1;
            }
            TIMELIB_ZONETYPE_ID => {
                (*time).zone_type = TIMELIB_ZONETYPE_ID;
                (*time).tz_info = (*parsed_zone).tz_info;
                (*time).is_localtime = 1;
                timezone = (*parsed_zone).tz_info;
            }
            _ => {}
        }
    }
    if !parsed_zone.is_null() {
        timelib_time_dtor(parsed_zone);
    }
    if !errors.is_null() {
        timelib_error_container_dtor(errors);
    }
    timezone
}

/// Parses and fully initializes a `createFromFormat()` value using the same
/// timelib fill/update sequence as php-src's `php_date_initialize()`.
pub fn create_from_format_serialized(
    format: &str,
    input: &str,
    base_timestamp: i64,
    timezone_name: &str,
) -> String {
    let (Ok(format), Ok(input), Ok(timezone_name)) = (
        CString::new(format),
        CString::new(input),
        CString::new(timezone_name),
    ) else {
        return parse_serialized(Some(format), input);
    };
    unsafe {
        let mut errors = std::ptr::null_mut();
        let parsed = timelib_parse_from_format(
            format.as_ptr(),
            input.as_ptr(),
            input.as_bytes().len(),
            &mut errors,
            timelib_builtin_db(),
            Some(cached_timezone_getter),
        );
        if parsed.is_null() || errors.is_null() || (*errors).error_count != 0 {
            let serialized = serialize_parsed(parsed, errors);
            if !parsed.is_null() {
                timelib_time_dtor(parsed);
            }
            if !errors.is_null() {
                timelib_error_container_dtor(errors);
            }
            return serialized;
        }

        let now = timelib_time_ctor();
        let timezone = attach_timezone(now, &timezone_name);
        timelib_unixtime2local(now, base_timestamp);
        (*now).us = 0;
        timelib_fill_holes(
            parsed,
            now,
            TIMELIB_NO_CLONE | TIMELIB_OVERRIDE_TIME,
        );
        timelib_update_ts(parsed, timezone);
        timelib_update_from_sse(parsed);
        (*parsed).have_relative = 0;

        let serialized = serialize_parsed(parsed, errors);
        timelib_time_dtor(parsed);
        timelib_time_dtor(now);
        timelib_error_container_dtor(errors);
        serialized
    }
}

/// Parses one free-form datetime and returns its Unix timestamp using php-src's
/// exact `strtotime()` fill/update sequence.
pub fn strtotime_timestamp(
    input: &str,
    base_timestamp: Option<i64>,
    timezone_name: &str,
) -> Option<i64> {
    if input.is_empty() {
        return None;
    }
    let input = CString::new(input).ok()?;
    let timezone_name = CString::new(timezone_name).ok()?;
    unsafe {
        let now = timelib_time_ctor();
        if now.is_null() {
            return None;
        }
        let timezone = attach_timezone(now, &timezone_name);
        if (*now).is_localtime == 0 {
            timelib_time_dtor(now);
            return None;
        }
        let base = base_timestamp.unwrap_or_else(current_unix_timestamp);
        timelib_unixtime2local(now, base);

        let mut errors = std::ptr::null_mut();
        let parsed = timelib_strtotime(
            input.as_ptr(),
            input.as_bytes().len(),
            &mut errors,
            timelib_builtin_db(),
            Some(cached_timezone_getter),
        );
        let failed = parsed.is_null()
            || errors.is_null()
            || (*errors).error_count != 0;
        if failed {
            if !parsed.is_null() {
                timelib_time_dtor(parsed);
            }
            if !errors.is_null() {
                timelib_error_container_dtor(errors);
            }
            timelib_time_dtor(now);
            return None;
        }
        timelib_fill_holes(parsed, now, TIMELIB_NO_CLONE);
        timelib_update_ts(parsed, timezone);
        let timestamp = (*parsed).sse;
        timelib_error_container_dtor(errors);
        timelib_time_dtor(parsed);
        timelib_time_dtor(now);
        Some(timestamp)
    }
}

/// Applies php-src's `DateTime::modify()` sequence to one zoned instant.
///
/// The first line is `O<TAB>timestamp<TAB>microsecond<TAB>reset-to-UTC` on
/// success or `E` on failure. Remaining lines contain the raw timelib parse
/// serialization so the PHP layer can update `DateTime::getLastErrors()`.
pub fn modify_serialized(
    timestamp: i64,
    microsecond: i64,
    timezone_name: &str,
    modifier: &str,
) -> Option<String> {
    let modifier = CString::new(modifier).ok()?;
    let timezone_name = CString::new(timezone_name).ok()?;
    unsafe {
        let base = timelib_time_ctor();
        if base.is_null() {
            return None;
        }
        attach_timezone(base, &timezone_name);
        timelib_unixtime2local(base, timestamp);
        (*base).us = microsecond;

        let mut errors = std::ptr::null_mut();
        let parsed = timelib_strtotime(
            modifier.as_ptr(),
            modifier.as_bytes().len(),
            &mut errors,
            timelib_builtin_db(),
            Some(cached_timezone_getter),
        );
        let parse_serialization = serialize_parsed(parsed, errors);
        if parsed.is_null() || errors.is_null() || (*errors).error_count != 0 {
            if !parsed.is_null() {
                timelib_time_dtor(parsed);
            }
            if !errors.is_null() {
                timelib_error_container_dtor(errors);
            }
            timelib_time_dtor(base);
            return Some(format!("E\n{parse_serialization}"));
        }

        (*base).relative = (*parsed).relative;
        (*base).have_relative = (*parsed).have_relative;
        (*base).sse_uptodate = 0;
        if (*parsed).y != TIMELIB_UNSET {
            (*base).y = (*parsed).y;
        }
        if (*parsed).m != TIMELIB_UNSET {
            (*base).m = (*parsed).m;
        }
        if (*parsed).d != TIMELIB_UNSET {
            (*base).d = (*parsed).d;
        }
        if (*parsed).h != TIMELIB_UNSET {
            (*base).h = (*parsed).h;
            if (*parsed).i != TIMELIB_UNSET {
                (*base).i = (*parsed).i;
                (*base).s = if (*parsed).s != TIMELIB_UNSET {
                    (*parsed).s
                } else {
                    0
                };
            } else {
                (*base).i = 0;
                (*base).s = 0;
            }
        }
        if (*parsed).us != TIMELIB_UNSET {
            (*base).us = (*parsed).us;
        }

        let reset_to_utc = (*parsed).y == 1970
            && (*parsed).m == 1
            && (*parsed).d == 1
            && (*parsed).h == 0
            && (*parsed).i == 0
            && (*parsed).s == 0
            && (*parsed).us == 0
            && (*parsed).have_zone != 0
            && (*parsed).zone_type == TIMELIB_ZONETYPE_OFFSET
            && (*parsed).z == 0
            && (*parsed).dst == 0;
        if reset_to_utc {
            timelib_set_timezone_from_offset(base, 0);
        }

        timelib_time_dtor(parsed);
        timelib_error_container_dtor(errors);
        timelib_update_ts(base, std::ptr::null_mut());
        timelib_update_from_sse(base);
        (*base).have_relative = 0;
        std::ptr::write_bytes(&mut (*base).relative, 0, 1);
        let output = format!(
            "O\t{}\t{}\t{}\n{}",
            (*base).sse,
            (*base).us,
            i64::from(reset_to_utc),
            parse_serialization,
        );
        timelib_time_dtor(base);
        Some(output)
    }
}

/// Serializes every relative-time field needed by DateInterval and interval
/// arithmetic as a tab-separated success record.
fn serialize_relative(relative: &TimelibRelativeTime) -> String {
    format!(
        "O\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
        relative.y,
        relative.m,
        relative.d,
        relative.h,
        relative.i,
        relative.s,
        relative.us,
        relative.invert,
        relative.days,
        relative.weekday,
        relative.weekday_behavior,
        relative.first_last_day_of,
        relative.special.type_,
        relative.special.amount,
        (relative.have_weekday_relative != 0) as u8
            | (((relative.have_special_relative != 0) as u8) << 1),
    )
}

/// Returns the first timelib parser error as an exact position/character/message
/// record used by DateInterval::createFromDateString().
unsafe fn serialize_first_interval_error(errors: *mut TimelibErrorContainer) -> String {
    if errors.is_null()
        || (*errors).error_count <= 0
        || (*errors).error_messages.is_null()
    {
        return "E\t0\t32\tThe timezone could not be found in the database".to_string();
    }
    let error = &*(*errors).error_messages;
    format!(
        "E\t{}\t{}\t{}",
        error.position,
        if error.character == 0 {
            32
        } else {
            error.character as u8
        },
        owned_c_string(error.message),
    )
}

/// Parses php-src's free-form grammar and returns its complete relative-time record.
///
/// `reject_non_relative` implements `DateInterval::createFromDateString()`, which rejects
/// otherwise-valid strings containing absolute date, time, or timezone fields. Serialization
/// restoration passes `false`: php-src accepts those fields there and clones only timelib's
/// relative sub-structure.
fn relative_interval_parse_serialized(input: &str, reject_non_relative: bool) -> String {
    let Ok(input_c) = CString::new(input) else {
        return "E\t0\t32\tUnexpected data found.".to_string();
    };
    unsafe {
        let mut errors = std::ptr::null_mut();
        let parsed = timelib_strtotime(
            input_c.as_ptr(),
            input.as_bytes().len(),
            &mut errors,
            timelib_builtin_db(),
            Some(cached_timezone_getter),
        );
        let result = if parsed.is_null()
            || errors.is_null()
            || (*errors).error_count != 0
        {
            serialize_first_interval_error(errors)
        } else if reject_non_relative
            && ((*parsed).have_date != 0
                || (*parsed).have_time != 0
                || (*parsed).have_zone != 0)
        {
            "N".to_string()
        } else {
            serialize_relative(&(*parsed).relative)
        };
        if !parsed.is_null() {
            timelib_time_dtor(parsed);
        }
        if !errors.is_null() {
            timelib_error_container_dtor(errors);
        }
        result
    }
}

/// Parses either a DateInterval ISO specification or php-src's free-form
/// relative grammar and returns its complete relative-time record.
pub fn interval_parse_serialized(input: &str, relative: bool) -> String {
    if relative {
        return relative_interval_parse_serialized(input, true);
    }
    let Ok(input_c) = CString::new(input) else {
        return "E\t0\t32\tUnexpected data found.".to_string();
    };
    unsafe {
        let mut begin = std::ptr::null_mut();
        let mut end = std::ptr::null_mut();
        let mut period = std::ptr::null_mut();
        let mut recurrences = 0;
        let mut errors = std::ptr::null_mut();
        timelib_strtointerval(
            input_c.as_ptr(),
            input.as_bytes().len(),
            &mut begin,
            &mut end,
            &mut period,
            &mut recurrences,
            &mut errors,
        );
        let result = if errors.is_null() || (*errors).error_count != 0 {
            "E".to_string()
        } else if !period.is_null() {
            serialize_relative(&*period)
        } else if !begin.is_null() && !end.is_null() {
            timelib_update_ts(begin, std::ptr::null_mut());
            timelib_update_ts(end, std::ptr::null_mut());
            let difference = timelib_diff(begin, end);
            if difference.is_null() {
                "F".to_string()
            } else {
                let serialized = serialize_relative(&*difference);
                timelib_rel_time_dtor(difference);
                serialized
            }
        } else {
            "F".to_string()
        };
        if !begin.is_null() {
            timelib_time_dtor(begin);
        }
        if !end.is_null() {
            timelib_time_dtor(end);
        }
        if !period.is_null() {
            timelib_rel_time_dtor(period);
        }
        if !errors.is_null() {
            timelib_error_container_dtor(errors);
        }
        result
    }
}

/// Parses a serialized `DateInterval::date_string` exactly as php-src's restoration path.
///
/// Unlike `createFromDateString()`, restoration accepts absolute fields and retains only the
/// relative sub-structure produced by timelib.
pub fn interval_restore_parse_serialized(input: &str) -> String {
    relative_interval_parse_serialized(input, false)
}

/// Parses the exact DatePeriod ISO grammar and serializes the optional start,
/// end, interval, and recurrence fields without imposing PHP's later validation.
pub fn period_parse_serialized(input: &str) -> String {
    let Ok(input_c) = CString::new(input) else {
        return "E".to_string();
    };
    unsafe {
        let mut begin = std::ptr::null_mut();
        let mut end = std::ptr::null_mut();
        let mut period = std::ptr::null_mut();
        let mut recurrences = 0;
        let mut errors = std::ptr::null_mut();
        timelib_strtointerval(
            input_c.as_ptr(),
            input.as_bytes().len(),
            &mut begin,
            &mut end,
            &mut period,
            &mut recurrences,
            &mut errors,
        );
        let result = if errors.is_null() || (*errors).error_count != 0 {
            "E".to_string()
        } else {
            if !begin.is_null() {
                timelib_update_ts(begin, std::ptr::null_mut());
            }
            if !end.is_null() {
                timelib_update_ts(end, std::ptr::null_mut());
            }
            let relative = if period.is_null() {
                TimelibRelativeTime {
                    y: 0,
                    m: 0,
                    d: 0,
                    h: 0,
                    i: 0,
                    s: 0,
                    us: 0,
                    weekday: 0,
                    weekday_behavior: 0,
                    first_last_day_of: 0,
                    invert: 0,
                    days: TIMELIB_UNSET,
                    special: TimelibRelativeSpecial { type_: 0, amount: 0 },
                    have_weekday_relative: 0,
                    have_special_relative: 0,
                }
            } else {
                *period
            };
            format!(
                "P\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                (!begin.is_null()) as u8,
                if begin.is_null() { 0 } else { (*begin).sse },
                (!end.is_null()) as u8,
                if end.is_null() { 0 } else { (*end).sse },
                (!period.is_null()) as u8,
                recurrences,
                relative.y,
                relative.m,
                relative.d,
                relative.h,
                relative.i,
            ) + &format!("\t{}\t{}", relative.s, relative.us)
        };
        if !begin.is_null() {
            timelib_time_dtor(begin);
        }
        if !end.is_null() {
            timelib_time_dtor(end);
        }
        if !period.is_null() {
            timelib_rel_time_dtor(period);
        }
        if !errors.is_null() {
            timelib_error_container_dtor(errors);
        }
        result
    }
}

/// Parses one integer field from an interval payload.
fn parse_payload_i64(value: Option<&str>) -> Option<i64> {
    value?.parse().ok()
}

/// Reconstructs the relative structure carried by an Elephc interval payload.
unsafe fn relative_from_payload(payload: &str) -> Option<(TimelibRelativeTime, bool)> {
    let kind = payload.as_bytes().first().copied()?;
    let fields = if kind == b'R' {
        let length_end = payload.find('\t')?;
        let raw_len: usize = payload.get(1..length_end)?.parse().ok()?;
        let raw_start = length_end + 1;
        let raw_end = raw_start.checked_add(raw_len)?;
        let raw = payload.get(raw_start..raw_end)?;
        let raw_c = CString::new(raw).ok()?;
        let mut errors = std::ptr::null_mut();
        let parsed = timelib_strtotime(
            raw_c.as_ptr(),
            raw.as_bytes().len(),
            &mut errors,
            timelib_builtin_db(),
            Some(cached_timezone_getter),
        );
        let valid = !parsed.is_null()
            && !errors.is_null()
            && (*errors).error_count == 0;
        if !valid {
            if !parsed.is_null() {
                timelib_time_dtor(parsed);
            }
            if !errors.is_null() {
                timelib_error_container_dtor(errors);
            }
            return None;
        }
        let relative = (*parsed).relative;
        timelib_time_dtor(parsed);
        timelib_error_container_dtor(errors);
        (relative, payload.get(raw_end + 1..)?)
    } else {
        (
            TimelibRelativeTime {
                y: 0,
                m: 0,
                d: 0,
                h: 0,
                i: 0,
                s: 0,
                us: 0,
                weekday: 0,
                weekday_behavior: 0,
                first_last_day_of: 0,
                invert: 0,
                days: TIMELIB_UNSET,
                special: TimelibRelativeSpecial { type_: 0, amount: 0 },
                have_weekday_relative: 0,
                have_special_relative: 0,
            },
            payload.get(2..)?,
        )
    };
    let (mut relative, fields) = fields;
    let mut values = fields.split('\t');
    relative.y = parse_payload_i64(values.next())?;
    relative.m = parse_payload_i64(values.next())?;
    relative.d = parse_payload_i64(values.next())?;
    relative.h = parse_payload_i64(values.next())?;
    relative.i = parse_payload_i64(values.next())?;
    relative.s = parse_payload_i64(values.next())?;
    relative.us = parse_payload_i64(values.next())?;
    relative.invert = parse_payload_i64(values.next())? as c_int;
    relative.days = parse_payload_i64(values.next())?;
    Some((relative, kind == b'W'))
}

/// Applies one serialized DateInterval through the exact timelib add/sub path
/// and returns `<timestamp>\t<microsecond>\t<special-sub-warning>`.
pub fn apply_interval_serialized(
    timestamp: i64,
    microsecond: i64,
    timezone_name: &str,
    payload: &str,
    subtract: bool,
) -> Option<String> {
    let timezone_name = CString::new(timezone_name).ok()?;
    unsafe {
        let (mut relative, wall) = relative_from_payload(payload)?;
        if subtract
            && (relative.have_weekday_relative != 0
                || relative.have_special_relative != 0)
        {
            return Some(format!("{timestamp}\t{microsecond}\t1"));
        }
        let base = timelib_time_ctor();
        if base.is_null() {
            return None;
        }
        attach_timezone(base, &timezone_name);
        timelib_unixtime2local(base, timestamp);
        (*base).us = microsecond;
        let result = match (subtract, wall) {
            (false, false) => timelib_add(base, &mut relative),
            (false, true) => timelib_add_wall(base, &mut relative),
            (true, false) => timelib_sub(base, &mut relative),
            (true, true) => timelib_sub_wall(base, &mut relative),
        };
        if result.is_null() {
            timelib_time_dtor(base);
            return None;
        }
        let serialized = format!("{}\t{}\t0", (*result).sse, (*result).us);
        timelib_time_dtor(result);
        timelib_time_dtor(base);
        Some(serialized)
    }
}

/// Replaces either the civil date or time fields of a fully zoned instant.
///
/// `payload` is `D<TAB>year<TAB>month<TAB>day` or
/// `T<TAB>hour<TAB>minute<TAB>second<TAB>microsecond`. Timelib owns PHP's
/// overflow normalization and historical timezone transition semantics.
pub fn set_civil_serialized(
    timestamp: i64,
    microsecond: i64,
    timezone_name: &str,
    payload: &str,
) -> Option<String> {
    let timezone_name = CString::new(timezone_name).ok()?;
    unsafe {
        let time = timelib_time_ctor();
        if time.is_null() {
            return None;
        }
        attach_timezone(time, &timezone_name);
        timelib_unixtime2local(time, timestamp);
        (*time).us = microsecond;

        let mut values = payload.split('\t');
        match values.next()? {
            "D" => {
                (*time).y = parse_payload_i64(values.next())?;
                (*time).m = parse_payload_i64(values.next())?;
                (*time).d = parse_payload_i64(values.next())?;
            }
            "T" => {
                (*time).h = parse_payload_i64(values.next())?;
                (*time).i = parse_payload_i64(values.next())?;
                (*time).s = parse_payload_i64(values.next())?;
                (*time).us = parse_payload_i64(values.next())?;
            }
            _ => {
                timelib_time_dtor(time);
                return None;
            }
        }
        timelib_update_ts(time, std::ptr::null_mut());
        let serialized = format!("{}\t{}", (*time).sse, (*time).us);
        timelib_time_dtor(time);
        Some(serialized)
    }
}

/// Applies php-src's `setISODate()` mutation and serializes both timestamp and civil state.
///
/// The civil fields are retained separately because timelib deliberately permits years whose
/// normalized timestamp wraps within `i64`; php-src continues formatting those original fields.
pub fn set_iso_date_serialized(
    timestamp: i64,
    microsecond: i64,
    timezone_name: &str,
    year: i64,
    week: i64,
    day: i64,
) -> Option<String> {
    let timezone_name = CString::new(timezone_name).ok()?;
    unsafe {
        let time = timelib_time_ctor();
        if time.is_null() {
            return None;
        }
        attach_timezone(time, &timezone_name);
        timelib_unixtime2local(time, timestamp);
        (*time).us = microsecond;
        (*time).y = year;
        (*time).m = 1;
        (*time).d = 1;
        (*time).relative = TimelibRelativeTime {
            y: 0,
            m: 0,
            d: timelib_daynr_from_weeknr(year, week, day),
            h: 0,
            i: 0,
            s: 0,
            us: 0,
            weekday: 0,
            weekday_behavior: 0,
            first_last_day_of: 0,
            invert: 0,
            // php-src zeroes the complete relative record before assigning `d`.
            days: 0,
            special: TimelibRelativeSpecial { type_: 0, amount: 0 },
            have_weekday_relative: 0,
            have_special_relative: 0,
        };
        (*time).have_relative = 1;
        timelib_update_ts(time, std::ptr::null_mut());
        let serialized = format!(
            "{}\t{}\t{}\t{}\t{}",
            (*time).sse,
            (*time).us,
            (*time).y,
            (*time).m,
            (*time).d,
        );
        timelib_time_dtor(time);
        Some(serialized)
    }
}

/// Computes php-src's exact civil difference between two fully zoned instants.
pub fn diff_serialized(
    left_timestamp: i64,
    left_microsecond: i64,
    left_timezone_name: &str,
    right_timestamp: i64,
    right_microsecond: i64,
    right_timezone_name: &str,
) -> Option<String> {
    let left_timezone_name = CString::new(left_timezone_name).ok()?;
    let right_timezone_name = CString::new(right_timezone_name).ok()?;
    unsafe {
        let left = timelib_time_ctor();
        let right = timelib_time_ctor();
        if left.is_null() || right.is_null() {
            if !left.is_null() {
                timelib_time_dtor(left);
            }
            if !right.is_null() {
                timelib_time_dtor(right);
            }
            return None;
        }
        attach_timezone(left, &left_timezone_name);
        timelib_unixtime2local(left, left_timestamp);
        (*left).us = left_microsecond;
        attach_timezone(right, &right_timezone_name);
        timelib_unixtime2local(right, right_timestamp);
        (*right).us = right_microsecond;
        let difference = timelib_diff(left, right);
        if difference.is_null() {
            timelib_time_dtor(left);
            timelib_time_dtor(right);
            return None;
        }
        let serialized = serialize_relative(&*difference);
        timelib_rel_time_dtor(difference);
        timelib_time_dtor(left);
        timelib_time_dtor(right);
        Some(serialized)
    }
}

/// Returns the current Unix timestamp without panicking before the epoch.
fn current_unix_timestamp() -> i64 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_secs().min(i64::MAX as u64) as i64,
        Err(error) => -(error.duration().as_secs().min(i64::MAX as u64) as i64),
    }
}

#[cfg(test)]
mod tests {
    //! Purpose:
    //! Checks that raw timelib serialization retains php-src field semantics.
    //!
    //! Called from:
    //! - `cargo test -p elephc-tz` through Rust's test harness.
    //!
    //! Key details:
    //! - Unset values stay at timelib's sentinel for PHP-side conversion to
    //!   `false`; relative and timezone records remain conditional.

    use super::*;

    /// A slash date keeps unspecified time fields unset, as `date_parse()` does.
    #[test]
    fn serializes_unset_free_form_fields() {
        let value = parse_serialized(None, "2024/06/15");
        assert!(value.starts_with(
            "P\t2024\t6\t15\t-9999999\t-9999999\t-9999999\t-9999999\t0\t0"
        ));
    }

    /// Date-only hyphen input leaves every time field unset for date_parse().
    #[test]
    fn serializes_unset_time_for_date_only_input() {
        let value = parse_serialized(None, "2024-03-15");
        assert!(
            value.starts_with(
                "P\t2024\t3\t15\t-9999999\t-9999999\t-9999999\t-9999999"
            ),
            "{value}"
        );
    }

    /// A date-only format leaves time and fraction unset for
    /// date_parse_from_format().
    #[test]
    fn serializes_unset_time_for_date_only_format() {
        let value = parse_serialized(Some("Y-m-d"), "2024-03-15");
        assert!(
            value.starts_with(
                "P\t2024\t3\t15\t-9999999\t-9999999\t-9999999\t-9999999"
            ),
            "{value}"
        );
    }

    /// Relative weekdays retain their relative record instead of being resolved.
    #[test]
    fn serializes_relative_weekday() {
        let value = parse_serialized(None, "next Monday");
        assert!(value.contains("\nR\t0\t0\t0\t0\t0\t0\t1\t-9999999\t0"));
    }

    /// Formatted parsing retains microseconds and fixed-offset zone metadata.
    #[test]
    fn serializes_formatted_fraction_and_zone() {
        let value = parse_serialized(
            Some("Y-m-d H:i:s.uP"),
            "2024-03-15 14:30:45.123456+02:00",
        );
        assert!(value.starts_with(
            "P\t2024\t3\t15\t14\t30\t45\t123456\t1\t1\t7200\t0"
        ));
    }

    /// Formatted initialization fills unset components, applies the timezone,
    /// and calculates the same Unix timestamp as php-src.
    #[test]
    fn initializes_create_from_format_timestamp() {
        let value = create_from_format_serialized(
            "Y-m-d H:i:s",
            "2024-03-15 14:30:45",
            0,
            "Europe/Paris",
        );
        let header: Vec<_> = value.lines().next().unwrap().split('\t').collect();
        assert_eq!(header[17], "1710509445");
        assert_eq!(header[9], "3");
        assert_eq!(header[13], "Europe/Paris");
    }

    /// ISO interval parsing preserves combined-representation components.
    #[test]
    fn parses_combined_interval_components() {
        let value = interval_parse_serialized("P0001-02-03T04:05:06", false);
        assert!(value.starts_with("O\t1\t2\t3\t4\t5\t6\t0\t0"));
    }

    /// Relative interval parsing preserves special weekday/month metadata and
    /// its observable basic month component.
    #[test]
    fn parses_special_relative_interval() {
        let value = interval_parse_serialized("first monday of next month", true);
        assert!(value.starts_with("O\t0\t1\t0\t0\t0\t0\t0\t0"));
        assert!(value.ends_with("\t3"));
    }

    /// DatePeriod parsing accepts both endpoints and a period.
    #[test]
    fn parses_dateperiod_endpoint_interval_form() {
        let value = period_parse_serialized(
            "2024-01-01T00:00:00Z/P2D/2024-01-07T00:00:00Z",
        );
        assert_eq!(
            value,
            "P\t1\t1704067200\t1\t1704585600\t1\t0\t0\t0\t2\t0\t0\t0\t0"
        );
    }

    /// POSIX runtime timezone names invert the sign while retaining optional
    /// minute and second precision.
    #[test]
    fn parses_posix_runtime_timezone_offsets() {
        assert_eq!(parse_posix_utc_offset("UTC-2"), Some(7_200));
        assert_eq!(parse_posix_utc_offset("UTC+5:30"), Some(-19_800));
        assert_eq!(parse_posix_utc_offset("UTC-2:30:45"), Some(9_045));
        assert_eq!(parse_posix_utc_offset("UTC-100"), None);
    }

    /// Free-form parsing accepts Elephc's internal POSIX representation for a
    /// PHP fixed-offset timezone.
    #[test]
    fn parses_datetime_in_posix_runtime_timezone() {
        assert_eq!(
            strtotime_timestamp(
                "2024-01-01 12:00:00",
                Some(0),
                "UTC-2",
            ),
            Some(1_704_103_200),
        );
    }

    /// Free-form parsing resolves canonical and backward-link timezone identifiers directly
    /// through php-src's timezone database before falling back to abbreviation parsing.
    #[test]
    fn parses_datetime_in_named_and_linked_timezones() {
        assert_eq!(
            strtotime_timestamp("1999-10-13", Some(0), "GMT0"),
            Some(939_772_800),
        );
        assert_eq!(
            strtotime_timestamp("Sep 04 16:39:45 2001", Some(0), "US/Eastern"),
            Some(999_635_985),
        );
    }

    /// `mktime()` retains php-src's full 64-bit year range instead of truncating through `tm_year`.
    #[test]
    fn computes_large_mktime_year() {
        assert_eq!(
            mktime_timestamp(0, 0, 0, 1, 1, 2_922_770_265, "America/Toronto"),
            Some(92_233_658_792_494_800),
        );
    }

    /// Historical local-mean-time offsets are applied before converting the civil clock.
    #[test]
    fn computes_historical_mktime_wall_clock() {
        assert_eq!(
            mktime_timestamp(1, 1, 1, 1, 1, 101, "America/Toronto"),
            Some(-58_979_900_487),
        );
        assert_eq!(
            mktime_timestamp(1, 1, 1, 1, 1, 101, "Europe/Oslo"),
            Some(-58_979_922_119),
        );
    }

    /// `setISODate(PHP_INT_MIN, 1, 1)` retains its non-reversible civil year beside the timestamp.
    #[test]
    fn computes_minimum_iso_date_civil_fields() {
        assert_eq!(
            set_iso_date_serialized(1_165_881_600, 0, "UTC", i64::MIN, 1, 1).as_deref(),
            Some("-62167170816\t0\t-9223372036854775808\t1\t2"),
        );
    }

    /// `modify()` copies timelib's parsed relative and absolute fields before DST normalization.
    #[test]
    fn modifies_fractional_relative_expression_across_dst_like_php_src() {
        let value = modify_serialized(
            1_711_796_400,
            250_000,
            "Europe/Paris",
            "+2 days 3 hours 4 minutes 5.500000 seconds",
        )
        .expect("modify serialization");
        assert!(value.starts_with("O\t1711954440\t0\t0\nP\t"), "{value}");
    }

    /// The special `@timestamp` modify form switches the object to a UTC offset zone.
    #[test]
    fn modifies_epoch_form_and_reports_timezone_reset() {
        let value = modify_serialized(1_711_796_400, 250_000, "Europe/Paris", "@0")
            .expect("epoch modify serialization");
        assert!(value.starts_with("O\t0\t0\t1\nP\t"), "{value}");
    }
}
