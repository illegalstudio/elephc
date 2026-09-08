//! Purpose:
//! Formats Unix timestamps with the vendored php-src timelib calendar and timezone data.
//! This is the overflow-safe fallback for timestamps outside libc `localtime()`/`gmtime()`.
//!
//! Called from:
//! - `crate::abi::elephc_tz_format()` through the compiled-program C ABI.
//!
//! Key details:
//! - Token behavior mirrors `ext/date/php_date.c::date_format()`.
//! - Calendar conversion supports the complete signed 64-bit timestamp range.

use std::os::raw::c_longlong;

use crate::timelib_ffi::{timestamp_parts, TimestampParts};

const DAY_SHORT: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const DAY_FULL: [&str; 7] = [
    "Sunday",
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
];
const MONTH_SHORT: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];
const MONTH_FULL: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

unsafe extern "C" {
    /// Returns timelib's Sunday-based weekday index for a civil date.
    fn timelib_day_of_week(year: c_longlong, month: c_longlong, day: c_longlong) -> c_longlong;
    /// Returns timelib's ISO Monday-based weekday index for a civil date.
    fn timelib_iso_day_of_week(
        year: c_longlong,
        month: c_longlong,
        day: c_longlong,
    ) -> c_longlong;
    /// Returns timelib's zero-based day-of-year index for a civil date.
    fn timelib_day_of_year(year: c_longlong, month: c_longlong, day: c_longlong) -> c_longlong;
    /// Returns the number of days in a civil month according to timelib's calendar rules.
    fn timelib_days_in_month(year: c_longlong, month: c_longlong) -> c_longlong;
    /// Writes the ISO week number and corresponding ISO week-year for a civil date.
    fn timelib_isoweek_from_date(
        year: c_longlong,
        month: c_longlong,
        day: c_longlong,
        week: *mut c_longlong,
        iso_year: *mut c_longlong,
    );
}

/// Formats a timestamp with php-src's byte-oriented `date()` token semantics.
pub(crate) fn format_timestamp(
    timestamp: i64,
    microsecond: i64,
    timezone_name: &str,
    format: &[u8],
    localtime: bool,
) -> Option<Vec<u8>> {
    let parts = timestamp_parts(timestamp, microsecond, timezone_name, localtime)?;
    format_parts(&parts, format)
}

/// Formats a timestamp while retaining separately persisted civil date fields.
///
/// `DateTime::setISODate()` can produce a timelib year outside the timestamp's
/// reversible range; php-src still formats that civil year/month/day verbatim.
pub(crate) fn format_civil_timestamp(
    timestamp: i64,
    microsecond: i64,
    timezone_name: &str,
    format: &[u8],
    localtime: bool,
    year: i64,
    month: i64,
    day: i64,
) -> Option<Vec<u8>> {
    let mut parts = timestamp_parts(timestamp, microsecond, timezone_name, localtime)?;
    parts.year = year;
    parts.month = month;
    parts.day = day;
    format_parts(&parts, format)
}

/// Collects PHP date output while preserving arbitrary literal bytes.
#[derive(Default)]
struct FormatBytes(Vec<u8>);

impl FormatBytes {
    /// Appends one ASCII format-token result or UTF-8 text fragment.
    fn push(&mut self, value: char) {
        let mut encoded = [0; 4];
        self.0
            .extend_from_slice(value.encode_utf8(&mut encoded).as_bytes());
    }

    /// Appends a token expansion whose generated text is valid UTF-8.
    fn push_str(&mut self, value: &str) {
        self.0.extend_from_slice(value.as_bytes());
    }

    /// Appends one literal PHP format byte without UTF-8 decoding.
    fn push_byte(&mut self, value: u8) {
        self.0.push(value);
    }

    /// Returns the completed PHP string payload.
    fn into_inner(self) -> Vec<u8> {
        self.0
    }
}

/// Formats already-normalized timelib parts with php-src's byte-oriented date semantics.
fn format_parts(parts: &TimestampParts, format: &[u8]) -> Option<Vec<u8>> {
    let mut output = FormatBytes::default();
    let mut index = 0;
    let mut iso = None;
    while index < format.len() {
        let byte = format[index];
        if !byte.is_ascii() {
            output.push_byte(byte);
            index += 1;
            continue;
        }
        let token = byte as char;
        if token == '\\' {
            index += 1;
            if index < format.len() {
                if format[index].is_ascii() {
                    output.push(format[index] as char);
                } else {
                    output.push_byte(format[index]);
                }
                index += 1;
            }
            continue;
        }
        append_token(&mut output, token, parts, &mut iso);
        index += 1;
    }
    Some(output.into_inner())
}

/// Appends one php-src date-format token or literal byte.
fn append_token(
    output: &mut FormatBytes,
    token: char,
    parts: &TimestampParts,
    iso: &mut Option<(i64, i64)>,
) {
    let day_of_week = || unsafe {
        timelib_day_of_week(parts.year, parts.month, parts.day)
    };
    let offset = timezone_offset(parts);
    let abbreviation = timezone_abbreviation(parts, offset);
    match token {
        'd' => output.push_str(&format!("{:02}", parts.day)),
        'D' => output.push_str(day_name(&DAY_SHORT, day_of_week())),
        'j' => output.push_str(&parts.day.to_string()),
        'l' => output.push_str(day_name(&DAY_FULL, day_of_week())),
        'S' => output.push_str(english_suffix(parts.day)),
        'w' => output.push_str(&day_of_week().to_string()),
        'N' => output.push_str(&unsafe {
            timelib_iso_day_of_week(parts.year, parts.month, parts.day)
        }
        .to_string()),
        'z' => output.push_str(&unsafe {
            timelib_day_of_year(parts.year, parts.month, parts.day)
        }
        .to_string()),
        'W' => output.push_str(&format!("{:02}", iso_week(parts, iso).0)),
        'o' => output.push_str(&iso_week(parts, iso).1.to_string()),
        'F' => output.push_str(month_name(&MONTH_FULL, parts.month)),
        'm' => output.push_str(&format!("{:02}", parts.month)),
        'M' => output.push_str(month_name(&MONTH_SHORT, parts.month)),
        'n' => output.push_str(&parts.month.to_string()),
        't' => output.push_str(&unsafe {
            timelib_days_in_month(parts.year, parts.month)
        }
        .to_string()),
        'L' => output.push(if is_leap_php_int(parts.year) { '1' } else { '0' }),
        'y' => output.push_str(&format!("{:02}", parts.year % 100)),
        'Y' => output.push_str(&expanded_year(parts.year, false, false)),
        'x' => output.push_str(&expanded_year(parts.year, true, false)),
        'X' => output.push_str(&expanded_year(parts.year, true, true)),
        'a' => output.push_str(if parts.hour >= 12 { "pm" } else { "am" }),
        'A' => output.push_str(if parts.hour >= 12 { "PM" } else { "AM" }),
        'B' => output.push_str(&format!("{:03}", swatch_beat(parts.timestamp))),
        'g' => output.push_str(&hour_12(parts.hour).to_string()),
        'G' => output.push_str(&parts.hour.to_string()),
        'h' => output.push_str(&format!("{:02}", hour_12(parts.hour))),
        'H' => output.push_str(&format!("{:02}", parts.hour)),
        'i' => output.push_str(&format!("{:02}", parts.minute)),
        's' => output.push_str(&format!("{:02}", parts.second)),
        'u' => output.push_str(&format!("{:06}", parts.microsecond)),
        'v' => output.push_str(&format!("{:03}", parts.microsecond / 1_000)),
        'I' => output.push(if parts.localtime && parts.dst != 0 { '1' } else { '0' }),
        'p' if !parts.localtime
            || matches!(abbreviation.as_str(), "UTC" | "Z" | "GMT+0000") =>
        {
            output.push('Z');
        }
        'p' | 'P' => output.push_str(&format_offset(offset, true)),
        'O' => output.push_str(&format_offset(offset, false)),
        'T' => output.push_str(if parts.localtime {
            &abbreviation
        } else {
            "GMT"
        }),
        'e' => output.push_str(&timezone_display_name(parts, offset, &abbreviation)),
        'Z' => output.push_str(&offset.to_string()),
        'c' => output.push_str(&format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}{}",
            parts.year,
            parts.month,
            parts.day,
            parts.hour,
            parts.minute,
            parts.second,
            format_offset(offset, true),
        )),
        'r' => output.push_str(&format!(
            "{}, {:02} {} {:04} {:02}:{:02}:{:02} {}",
            day_name(&DAY_SHORT, day_of_week()),
            parts.day,
            month_name(&MONTH_SHORT, parts.month),
            parts.year,
            parts.hour,
            parts.minute,
            parts.second,
            format_offset(offset, false),
        )),
        'U' => output.push_str(&parts.timestamp.to_string()),
        literal => output.push(literal),
    }
}

/// Returns a month name or php-src's defensive empty fallback.
fn month_name<'a>(names: &'a [&str; 12], month: i64) -> &'a str {
    usize::try_from(month - 1)
        .ok()
        .and_then(|index| names.get(index).copied())
        .unwrap_or("")
}

/// Returns a weekday name or php-src's `"Unknown"` fallback.
fn day_name<'a>(names: &'a [&str; 7], weekday: i64) -> &'a str {
    usize::try_from(weekday)
        .ok()
        .and_then(|index| names.get(index).copied())
        .unwrap_or("Unknown")
}

/// Returns the English ordinal suffix for a day of month.
fn english_suffix(day: i64) -> &'static str {
    if (11..=13).contains(&(day % 100)) {
        return "th";
    }
    match day % 10 {
        1 => "st",
        2 => "nd",
        3 => "rd",
        _ => "th",
    }
}

/// Returns the ISO week and ISO week-year, caching the timelib calculation.
fn iso_week(parts: &TimestampParts, cached: &mut Option<(i64, i64)>) -> (i64, i64) {
    *cached.get_or_insert_with(|| {
        let mut week = 0;
        let mut year = 0;
        unsafe {
            timelib_isoweek_from_date(
                parts.year,
                parts.month,
                parts.day,
                &mut week,
                &mut year,
            );
        }
        (week, year)
    })
}

/// Implements php-src's Gregorian leap-year predicate after its C `int` narrowing.
fn is_leap_php_int(year: i64) -> bool {
    let year = i64::from(year as i32);
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

/// Formats PHP's `Y`, `x`, and `X` expanded-year tokens.
fn expanded_year(year: i64, expanded: bool, always_sign: bool) -> String {
    let sign = if year < 0 {
        "-"
    } else if always_sign || (expanded && year >= 10_000) {
        "+"
    } else {
        ""
    };
    format!("{sign}{:04}", year.unsigned_abs())
}

/// Converts a 24-hour value to PHP's 1–12 clock field.
fn hour_12(hour: i64) -> i64 {
    let value = hour % 12;
    if value == 0 { 12 } else { value }
}

/// Computes the zero-padded Swatch Internet Time beat.
fn swatch_beat(timestamp: i64) -> i64 {
    let mut value = ((timestamp % 86_400) + 3_600) * 10;
    if value < 0 {
        value += 864_000;
    }
    (value / 864) % 1_000
}

/// Returns the east-of-UTC offset php-src exposes for the attached zone type.
fn timezone_offset(parts: &TimestampParts) -> i64 {
    if !parts.localtime {
        return 0;
    }
    if parts.zone_type == 2 {
        parts.offset + parts.dst * 3_600
    } else {
        parts.offset
    }
}

/// Returns php-src's display abbreviation, including generated `GMT±HHMM` offset names.
fn timezone_abbreviation(parts: &TimestampParts, offset: i64) -> String {
    if !parts.localtime {
        return "GMT".to_string();
    }
    if parts.zone_type == 1 {
        return format!(
            "GMT{}{:02}{:02}",
            if offset < 0 { '-' } else { '+' },
            offset.abs() / 3_600,
            (offset.abs() % 3_600) / 60,
        );
    }
    parts.abbreviation.clone()
}

/// Formats a numeric UTC offset for PHP's `O`/`P` tokens.
fn format_offset(offset: i64, colon: bool) -> String {
    format!(
        "{}{:02}{}{:02}",
        if offset < 0 { '-' } else { '+' },
        offset.abs() / 3_600,
        if colon { ":" } else { "" },
        (offset.abs() % 3_600) / 60,
    )
}

/// Returns PHP's `e` timezone identifier for ID, abbreviation, and offset zones.
fn timezone_display_name(parts: &TimestampParts, offset: i64, abbreviation: &str) -> String {
    if !parts.localtime {
        return "UTC".to_string();
    }
    match parts.zone_type {
        1 => {
            let seconds = offset.abs() % 60;
            let base = format_offset(offset, true);
            if seconds == 0 {
                base
            } else {
                format!("{base}:{seconds:02}")
            }
        }
        2 => abbreviation.to_string(),
        3 => parts.timezone_id.clone(),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    //! Purpose:
    //! Verifies overflow-safe php-src date formatting against signed timestamp boundaries.
    //!
    //! Called from:
    //! - `cargo test -p elephc-tz` through Rust's test harness.
    //!
    //! Key details:
    //! - Expected strings come from the audited php-src `bug75851.phpt`.

    use super::*;

    /// Formats PHP_INT_MIN without libc calendar overflow.
    #[test]
    fn formats_minimum_timestamp() {
        assert_eq!(
            format_timestamp(
                i64::MIN,
                0,
                "UTC",
                b"c\nr\no\ny\nY\nU",
                true,
            )
            .as_deref(),
            Some(
                b"-292277022657-01-27T08:29:52+00:00\nSun, 27 Jan -292277022657 08:29:52 +0000\n-292277022657\n-57\n-292277022657\n-9223372036854775808".as_slice()
            ),
        );
    }

    /// Formats PHP_INT_MAX without libc calendar overflow.
    #[test]
    fn formats_maximum_timestamp() {
        assert_eq!(
            format_timestamp(i64::MAX, 0, "UTC", b"Y-m-d H:i:s U", true).as_deref(),
            Some(b"292277026596-12-04 15:30:07 9223372036854775807".as_slice()),
        );
    }

    /// Preserves php-src's pre-transition Amsterdam local mean time conversion.
    #[test]
    fn formats_pre_transition_amsterdam_timestamp() {
        assert_eq!(
            format_timestamp(
                -59_042_996_372,
                0,
                "Europe/Amsterdam",
                b"Y-m-d H:i:s P T",
                true,
            )
            .as_deref(),
            Some(b"0099-01-01 00:00:00 +00:19 LMT".as_slice()),
        );
    }

    /// Preserves multibyte UTF-8 literals instead of re-encoding each source byte as Latin-1.
    #[test]
    fn preserves_utf8_format_literals() {
        assert_eq!(
            format_timestamp(0, 0, "UTC", "あ\\い".as_bytes(), true).as_deref(),
            Some("あい".as_bytes()),
        );
    }

    /// Separately retained civil fields remain visible when their timestamp maps back to year zero.
    #[test]
    fn formats_minimum_iso_civil_year() {
        assert_eq!(
            format_civil_timestamp(
                -62_167_170_816,
                0,
                "UTC",
                b"Y|x|X|m|d",
                true,
                i64::MIN,
                1,
                2,
            )
            .as_deref(),
            Some(
                b"-9223372036854775808|-9223372036854775808|-9223372036854775808|01|02".as_slice()
            ),
        );
    }

    /// Preserves NUL and non-UTF-8 literal date-format bytes exactly.
    #[test]
    fn preserves_binary_format_literals() {
        assert_eq!(
            format_timestamp(0, 0, "UTC", b"\0\xff", true).as_deref(),
            Some(b"\0\xff".as_slice()),
        );
    }

    /// Narrows civil years to php-src's C `int` for the `L` leap-year token.
    #[test]
    fn leap_token_uses_php_int_year_narrowing() {
        assert_eq!(
            format_civil_timestamp(0, 0, "UTC", b"L", true, 4_294_967_396, 1, 1)
                .as_deref(),
            Some(b"0".as_slice()),
        );
    }
}
