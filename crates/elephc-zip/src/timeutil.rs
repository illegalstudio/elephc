//! Purpose:
//! Converts between Unix timestamps and the civil date fields ZIP stores.
//!
//! Called from:
//! - `crate::metadata::dos_to_unix` and `crate::write` when stamping DOS/extended times.
//!
//! Key details:
//! - Pure integer arithmetic; no host timezone or libc dependency.

/// Converts a civil UTC date/time into seconds since 1970-01-01.
pub(crate) fn civil_to_unix(year: i64, month: i64, day: i64, hour: i64, minute: i64, second: i64) -> i64 {
    let years = year - if month <= 2 { 1 } else { 0 };
    let era = if years >= 0 { years } else { years - 399 } / 400;
    let yoe = years - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    days * 86_400 + hour * 3_600 + minute * 60 + second
}

/// Converts seconds since the epoch into UTC civil fields.
pub(crate) fn unix_to_civil(timestamp: i64) -> (i64, i64, i64, i64, i64, i64) {
    let days = timestamp.div_euclid(86_400);
    let seconds = timestamp.rem_euclid(86_400);
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let mut year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    if month <= 2 { year += 1; }
    (year, month, day, seconds / 3_600, seconds % 3_600 / 60, seconds % 60)
}
