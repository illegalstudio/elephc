//! Purpose:
//! Checked civil-time conversion for native PHP mktime/gmmktime callables.
//!
//! Called from:
//! - Generated code through the two checked C ABI entry points.
//!
//! Key details:
//! - Bits 1..5 mark nullable optional fields; hour never receives a clock default.
//! - Two integer return words distinguish a valid timestamp -1 from failure.

use std::time::{SystemTime, UNIX_EPOCH};

/// Two INTEGER-class C ABI result words on all supported AArch64/System V targets.
#[repr(C)]
#[derive(Debug, PartialEq, Eq)]
pub struct MktimeResult {
    /// Unix timestamp, meaningful only when `valid` is one.
    pub timestamp: i64,
    /// One for an integer result, zero for PHP false.
    pub valid: i64,
}

/// Converts civil fields using one optional clock snapshot and the explicit timezone.
fn checked_mktime(
    mut fields: [i64; 6],
    null_mask: u64,
    timezone: &str,
    clock: impl FnOnce() -> Option<i64>,
) -> MktimeResult {
    let result = (|| {
        if null_mask & !0b11_1110 != 0 {
            return None;
        }
        if null_mask != 0 {
            let now = clock()?;
            let bytes = crate::format_timestamp_php(now, timezone, b"G\ti\ts\tn\tj\tY", true)?;
            let defaults = bytes.split(|byte| *byte == b'\t')
                .map(|field| std::str::from_utf8(field).ok()?.parse::<i64>().ok())
                .collect::<Option<Vec<_>>>()?;
            if defaults.len() != fields.len() {
                return None;
            }
            for index in 1..fields.len() {
                if null_mask & (1 << index) != 0 {
                    fields[index] = defaults[index];
                }
            }
        }
        crate::mktime_timestamp_php(fields[0], fields[1], fields[2], fields[3], fields[4], fields[5], timezone)
    })();
    match result {
        Some(timestamp) => MktimeResult { timestamp, valid: 1 },
        None => MktimeResult { timestamp: 0, valid: 0 },
    }
}

/// Reads floor-rounded Unix seconds, including a clock set before the epoch.
fn unix_seconds() -> Option<i64> {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => i64::try_from(duration.as_secs()).ok(),
        Err(error) => {
            let duration = error.duration();
            i64::try_from(duration.as_secs()).ok()?.checked_neg()?
                .checked_sub(i64::from(duration.subsec_nanos() != 0))
        }
    }
}

/// C ABI: converts local fields, defaulting nullable optionals in the active timezone.
#[no_mangle]
pub extern "C" fn elephc_tz_mktime_checked(
    hour: i64, minute: i64, second: i64, month: i64, day: i64, year: i64, null_mask: u64,
) -> MktimeResult {
    let timezone = std::env::var("TZ").unwrap_or_else(|_| "UTC".to_owned());
    checked_mktime([hour, minute, second, month, day, year], null_mask, &timezone, unix_seconds)
}

/// C ABI: converts UTC fields with the same checked result and nullable-field mask.
#[no_mangle]
pub extern "C" fn elephc_tz_gmmktime_checked(
    hour: i64, minute: i64, second: i64, month: i64, day: i64, year: i64, null_mask: u64,
) -> MktimeResult {
    checked_mktime([hour, minute, second, month, day, year], null_mask, "UTC", unix_seconds)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The native return is exactly two aligned integer words, with no hidden payload.
    #[test]
    fn checked_mktime_result_layout() {
        assert_eq!(std::mem::size_of::<MktimeResult>(), 16);
        assert_eq!(std::mem::align_of::<MktimeResult>(), 8);
        assert_eq!(std::mem::offset_of!(MktimeResult, timestamp), 0);
        assert_eq!(std::mem::offset_of!(MktimeResult, valid), 8);
    }

    /// A real -1 timestamp remains an integer rather than the failure sentinel.
    #[test]
    fn checked_mktime_distinguishes_minus_one_and_failure() {
        let result = checked_mktime([23, 59, 59, 12, 31, 1969], 0, "UTC", || panic!("no defaults"));
        assert_eq!(result, MktimeResult { timestamp: -1, valid: 1 });
        assert_eq!(checked_mktime([0; 6], 2, "UTC", || None).valid, 0);
        assert_eq!(checked_mktime([0; 6], 1, "UTC", || panic!("invalid mask")).valid, 0);
    }

    /// All five nullable fields use one coherent timestamp, including year boundaries.
    #[test]
    fn checked_mktime_defaults_share_one_clock_sample() {
        for timezone in ["UTC", "Europe/Paris", "Asia/Kolkata"] {
            for timestamp in [-1, 1_704_067_199, 1_704_067_200] {
                let hour = crate::format_timestamp_php(timestamp, timezone, b"G", true).unwrap();
                let hour = std::str::from_utf8(&hour).unwrap().parse().unwrap();
                let mut reads = 0;
                let result = checked_mktime([hour, 0, 0, 0, 0, 0], 0b11_1110, timezone, || {
                    reads += 1;
                    Some(timestamp)
                });
                assert_eq!(reads, 1);
                assert_eq!(result, MktimeResult { timestamp, valid: 1 });
            }
        }
    }
}
