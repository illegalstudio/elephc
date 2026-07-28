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
//!   commit; compile-time size assertions guard the 64-bit supported target
//!   matrix against accidental ABI drift.
//! - Parsing does not fill unset fields. This is deliberate: php-src's
//!   `php_date_do_return_parsed_time()` exposes the raw parse structure.

use std::collections::HashMap;
use std::ffi::{CStr, CString};
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
struct TimelibTimezoneInfo {
    name: *mut c_char,
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
    assert!(std::mem::size_of::<TimelibRelativeSpecial>() == 16);
    assert!(std::mem::size_of::<TimelibRelativeTime>() == 104);
    assert!(std::mem::size_of::<TimelibTime>() == 240);
    assert!(std::mem::size_of::<TimelibErrorMessage>() == 24);
    assert!(std::mem::size_of::<TimelibErrorContainer>() == 24);
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

/// Parses either a DateInterval ISO specification or php-src's free-form
/// relative grammar and returns its complete relative-time record.
pub fn interval_parse_serialized(input: &str, relative: bool) -> String {
    let Ok(input_c) = CString::new(input) else {
        return "E\t0\t32\tUnexpected data found.".to_string();
    };
    unsafe {
        if relative {
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
            } else if (*parsed).have_date != 0
                || (*parsed).have_time != 0
                || (*parsed).have_zone != 0
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
            return result;
        }

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
            && (*errors).error_count == 0
            && (*parsed).have_date == 0
            && (*parsed).have_time == 0
            && (*parsed).have_zone == 0;
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
}
