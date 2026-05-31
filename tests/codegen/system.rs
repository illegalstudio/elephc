//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of system, including date year, date full format, and date time format.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use crate::support::*;

// --- Date/time functions ---

/// Verifies `date("Y", timestamp)` returns the correct 4-digit year for a known UTC timestamp.
#[test]
fn test_date_year() {
    let out = compile_and_run("<?php echo date(\"Y\", 1700000000);");
    assert_eq!(out, "2023");
}

/// Verifies `date("Y-m-d", timestamp)` formats a date as ISO 8601 date string for a known UTC timestamp.
#[test]
fn test_date_full_format() {
    let out = compile_and_run("<?php echo date(\"Y-m-d\", 1700000000);");
    assert_eq!(out, "2023-11-14");
}

/// Verifies `date("H:i:s", timestamp)` formats time as zero-padded HH:MM:SS; exact
/// output is timezone-dependent so the test validates the format pattern rather than the value.
#[test]
fn test_date_time_format() {
    let out = compile_and_run("<?php echo date(\"H:i:s\", 1700000000);");
    // The exact output depends on the timezone, but it should have the format HH:MM:SS
    let out_trimmed = out.trim();
    assert_eq!(out_trimmed.len(), 8);
    assert_eq!(&out_trimmed[2..3], ":");
    assert_eq!(&out_trimmed[5..6], ":");
}

/// Verifies `date("j", timestamp)` day-of-month without zero padding is in range 1–31.
#[test]
fn test_date_day_no_padding() {
    let out = compile_and_run("<?php echo date(\"j\", 1700000000);");
    let val: i32 = out.trim().parse().unwrap();
    assert!(val >= 1 && val <= 31);
}

/// Verifies `date("A", timestamp)` returns uppercase "AM" or "PM" for a known UTC timestamp.
#[test]
fn test_date_am_pm() {
    let out = compile_and_run("<?php echo date(\"A\", 1700000000);");
    assert!(out == "AM" || out == "PM");
}

/// Verifies `date("a", timestamp)` returns lowercase "am" or "pm" for a known UTC timestamp.
#[test]
fn test_date_am_pm_lower() {
    let out = compile_and_run("<?php echo date(\"a\", 1700000000);");
    assert!(out == "am" || out == "pm");
}

/// Verifies `date("U", timestamp)` returns the raw Unix timestamp as a decimal string.
#[test]
fn test_date_unix_timestamp() {
    let out = compile_and_run("<?php echo date(\"U\", 1700000000);");
    assert_eq!(out, "1700000000");
}

/// Verifies `date("D", timestamp)` returns a 3-letter abbreviated weekday name for a known UTC timestamp.
#[test]
fn test_date_short_day() {
    let out = compile_and_run("<?php echo date(\"D\", 1700000000);");
    let valid_days = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
    assert!(valid_days.contains(&out.as_str()), "Got: {}", out);
}

/// Verifies `date("M", timestamp)` returns the 3-letter abbreviated month name for a known UTC timestamp (Nov).
#[test]
fn test_date_short_month() {
    let out = compile_and_run("<?php echo date(\"M\", 1700000000);");
    assert_eq!(out, "Nov");
}

/// Verifies `date("N", timestamp)` returns ISO 8601 day of week (1=Monday … 7=Sunday) in range 1–7.
#[test]
fn test_date_iso_day_of_week() {
    let out = compile_and_run("<?php echo date(\"N\", 1700000000);");
    let val: i32 = out.trim().parse().unwrap();
    assert!(val >= 1 && val <= 7);
}

/// Verifies `date("g", timestamp)` returns 12-hour hour without zero padding (1–12) for a known UTC timestamp.
#[test]
fn test_date_12_hour() {
    let out = compile_and_run("<?php echo date(\"g\", 1700000000);");
    let val: i32 = out.trim().parse().unwrap();
    assert!(val >= 1 && val <= 12);
}

/// Verifies `date()` accepts literal characters (e.g. "/") in the format string and passes them through verbatim.
#[test]
fn test_date_literal_text() {
    let out = compile_and_run("<?php echo date(\"Y/m/d\", 1700000000);");
    assert_eq!(out, "2023/11/14");
}

/// Verifies `mktime()` constructs a timestamp for a given date (2000-01-01 00:00:00) and
/// `date("Y-m-d")` formats it back to the same date.
#[test]
fn test_mktime() {
    let out = compile_and_run(
        "<?php
$ts = mktime(0, 0, 0, 1, 1, 2000);
echo date(\"Y-m-d\", $ts);
",
    );
    assert_eq!(out, "2000-01-01");
}

/// Verifies `mktime(hour, minute, second, month, day, year)` preserves the full time down to the second.
#[test]
fn test_mktime_specific_time() {
    let out = compile_and_run(
        "<?php
$ts = mktime(12, 30, 45, 6, 15, 2024);
echo date(\"H:i:s\", $ts);
",
    );
    assert_eq!(out, "12:30:45");
}

/// Verifies `strtotime("YYYY-MM-DD")` parses a plain ISO date string.
#[test]
fn test_strtotime_date() {
    let out = compile_and_run(
        r#"<?php
$ts = strtotime("2000-01-01");
echo date("Y-m-d", $ts);
"#,
    );
    assert_eq!(out, "2000-01-01");
}

/// Verifies `strtotime("YYYY-MM-DD HH:MM:SS")` parses a full datetime with seconds.
#[test]
fn test_strtotime_datetime() {
    let out = compile_and_run(
        r#"<?php
$ts = strtotime("2024-06-15 12:30:45");
echo date("Y-m-d H:i:s", $ts);
"#,
    );
    assert_eq!(out, "2024-06-15 12:30:45");
}

/// Verifies `strtotime("YYYY-MM-DD HH:MM")` (no seconds) pads seconds to :00.
#[test]
fn test_strtotime_datetime_without_seconds() {
    let out = compile_and_run(
        r#"<?php
$ts = strtotime("2024-06-15 12:30");
echo date("Y-m-d H:i:s", $ts);
"#,
    );
    assert_eq!(out, "2024-06-15 12:30:00");
}

/// Verifies `strtotime` accepts both uppercase "T" and lowercase "t" as datetime separators.
#[test]
fn test_strtotime_datetime_t_separator() {
    let out = compile_and_run(
        r#"<?php
$upper = strtotime("2024-06-15T12:00:00");
$lower = strtotime("2024-06-15t12:30");
echo date("Y-m-d H:i:s", $upper) . ",";
echo date("Y-m-d H:i:s", $lower);
"#,
    );
    assert_eq!(out, "2024-06-15 12:00:00,2024-06-15 12:30:00");
}

/// Verifies `strtotime` returns -1 for malformed ISO-like strings that have extra junk after the datetime.
#[test]
fn test_strtotime_rejects_malformed_iso_datetime() {
    let out = compile_and_run(
        r#"<?php
echo strtotime("2024-06-15 12:30:45 extra") . ",";
echo strtotime("2024-06-15abc") . ",";
echo strtotime("2024-06-15 12:30x") . ",";
echo strtotime("2024-06-15 12") . ",";
echo strtotime("2024/06/15") . ",";
echo strtotime("2024-0x-15");
"#,
    );
    assert_eq!(out, "-1,-1,-1,-1,-1,-1");
}

/// Verifies a round-trip: `mktime` and `strtotime("YYYY-MM-DD HH:MM:SS")` produce identical timestamps.
#[test]
fn test_strtotime_mktime_roundtrip() {
    let out = compile_and_run(
        r#"<?php
$ts1 = mktime(10, 30, 0, 3, 25, 2024);
$ts2 = strtotime("2024-03-25 10:30:00");
if ($ts1 == $ts2) {
    echo "match";
}
"#,
    );
    assert_eq!(out, "match");
}

/// Verifies `strtotime("now")` returns a timestamp within a few seconds of the current time.
#[test]
fn test_strtotime_now() {
    let out = compile_and_run(
        r#"<?php
$t = time();
$s = strtotime("now");
if ($s >= $t - 2 && $s <= $t + 2) echo "ok";
"#,
    );
    assert_eq!(out, "ok");
}

/// Verifies `strtotime` is case-insensitive: "NOW" (uppercase) is equivalent to "now".
#[test]
fn test_strtotime_now_uppercase() {
    let out = compile_and_run(
        r#"<?php
$t = time();
$s = strtotime("NOW");
if ($s >= $t - 2 && $s <= $t + 2) echo "ok";
"#,
    );
    assert_eq!(out, "ok");
}

/// Verifies `strtotime("today")` resolves to 00:00:00 on the current day.
#[test]
fn test_strtotime_today_midnight() {
    let out = compile_and_run(
        r#"<?php
$ts = strtotime("today");
echo date("H:i:s", $ts);
"#,
    );
    assert_eq!(out, "00:00:00");
}

/// Verifies `strtotime("midnight")` resolves to 00:00:00 on the current day.
#[test]
fn test_strtotime_midnight() {
    let out = compile_and_run(
        r#"<?php
$ts = strtotime("midnight");
echo date("H:i:s", $ts);
"#,
    );
    assert_eq!(out, "00:00:00");
}

/// Verifies `strtotime` trims leading and trailing ASCII whitespace around the input string.
#[test]
fn test_strtotime_trims_ascii_whitespace() {
    let out = compile_and_run(
        "<?php $ts = strtotime(\"\\n\\t today \\n\"); echo date(\"H:i:s\", $ts);",
    );
    assert_eq!(out, "00:00:00");
}

/// Verifies `strtotime("tomorrow")` produces a timestamp 82800–90000 seconds ahead of "today"
/// (allowing ±1 hour for DST transitions).
#[test]
fn test_strtotime_tomorrow() {
    let out = compile_and_run(
        r#"<?php
$today = strtotime("today");
$tomorrow = strtotime("tomorrow");
$diff = $tomorrow - $today;
if ($diff >= 82800 && $diff <= 90000) echo "ok";
"#,
    );
    assert_eq!(out, "ok");
}

/// Verifies `strtotime("yesterday")` produces a timestamp 82800–90000 seconds behind "today"
/// (allowing ±1 hour for DST transitions).
#[test]
fn test_strtotime_yesterday() {
    let out = compile_and_run(
        r#"<?php
$today = strtotime("today");
$yesterday = strtotime("yesterday");
$diff = $today - $yesterday;
if ($diff >= 82800 && $diff <= 90000) echo "ok";
"#,
    );
    assert_eq!(out, "ok");
}

/// Verifies `strtotime("noon")` resolves to 12:00:00 on the current day.
#[test]
fn test_strtotime_noon() {
    let out = compile_and_run(
        r#"<?php
$ts = strtotime("noon");
echo date("H:i:s", $ts);
"#,
    );
    assert_eq!(out, "12:00:00");
}

/// Verifies `strtotime` weekday keywords are case-insensitive: "Noon" (capitalized) is equivalent to "noon".
#[test]
fn test_strtotime_noon_capitalized() {
    let out = compile_and_run(
        r#"<?php
$ts = strtotime("Noon");
echo date("H:i:s", $ts);
"#,
    );
    assert_eq!(out, "12:00:00");
}

/// Regression: ensures `time()` followed by `date()` does not crash due to a macOS TLS/tzset
/// interaction. A raw `gettimeofday` syscall skipped libc's lazy TLS init, causing `localtime`
/// to crash in `__findenv_locked` when `tzset` first read `environ`. The fix routes `__rt_time`
/// through libc `time(NULL)` on macOS arm64.
#[test]
fn test_time_then_localtime_regression() {
    // Regression: a raw macOS gettimeofday syscall in __rt_time used to skip libc's
    // lazy TLS/__findenv init, so any subsequent `localtime` chain crashed in
    // `__findenv_locked` when `tzset` first read `environ`. __rt_time now routes
    // through libc `time(NULL)` on macOS arm64 to keep that init coherent.
    let out = compile_and_run(
        r#"<?php
$now = time();
echo date("Y", $now);
"#,
    );
    let val: i32 = out.trim().parse().unwrap();
    assert!(val >= 2024, "expected current year >= 2024, got {}", out);
}

/// Verifies `strtotime` returns -1 for a non-parseable string like "garbage".
#[test]
fn test_strtotime_invalid() {
    let out = compile_and_run(
        r#"<?php
$ts = strtotime("garbage");
echo $ts;
"#,
    );
    assert_eq!(out, "-1");
}

/// Verifies `strtotime` rejects keywords and weekday names with trailing junk characters
/// (e.g. "today123", "today!", "Monday2", "next Monday2") by returning -1.
#[test]
fn test_strtotime_rejects_keyword_and_weekday_suffix_junk() {
    let out = compile_and_run(
        r#"<?php
echo strtotime("today123") . ",";
echo strtotime("today!") . ",";
echo strtotime("Monday2") . ",";
echo strtotime("next Monday2");
"#,
    );
    assert_eq!(out, "-1,-1,-1,-1");
}

/// Verifies `strtotime("HH:MM")` (time-only, no seconds) parses and normalizes to HH:MM:00.
#[test]
fn test_strtotime_time_only_hhmm() {
    let out = compile_and_run(
        r#"<?php
$ts = strtotime("14:30");
echo date("H:i:s", $ts);
"#,
    );
    assert_eq!(out, "14:30:00");
}

/// Verifies `strtotime("HH:MM:SS")` (time-only with seconds) preserves the full time.
#[test]
fn test_strtotime_time_only_hhmmss() {
    let out = compile_and_run(
        r#"<?php
$ts = strtotime("09:15:42");
echo date("H:i:s", $ts);
"#,
    );
    assert_eq!(out, "09:15:42");
}

/// Verifies `strtotime` accepts single-digit hours (e.g. "9:30") and zero-pads to two digits.
#[test]
fn test_strtotime_time_only_single_digit_hour() {
    let out = compile_and_run(
        r#"<?php
$ts = strtotime("9:30");
echo date("H:i:s", $ts);
"#,
    );
    assert_eq!(out, "09:30:00");
}

/// Verifies `strtotime` returns -1 for invalid time-only shapes (junk suffix, out-of-range values, malformed separator).
#[test]
fn test_strtotime_rejects_invalid_time_only_shapes() {
    let out = compile_and_run(
        r#"<?php
echo strtotime("14:30abc") . ",";
echo strtotime("14:30:99") . ",";
echo strtotime("99:99") . ",";
echo strtotime("14:30:");
"#,
    );
    assert_eq!(out, "-1,-1,-1,-1");
}

/// Verifies `strtotime` follows PHP's permissive upper-bound behavior for time-only inputs:
/// "24:59:60" is accepted and wraps to 01:00:00 the next day.
#[test]
fn test_strtotime_time_only_php_upper_bounds() {
    let out = compile_and_run(
        r#"<?php
$ts = strtotime("24:59:60");
echo date("H:i:s", $ts);
"#,
    );
    assert_eq!(out, "01:00:00");
}

/// Verifies `strtotime("+1 hour")` produces a timestamp roughly 3600 seconds ahead of now (±10s tolerance).
#[test]
fn test_strtotime_offset_plus_one_hour() {
    let out = compile_and_run(
        r#"<?php
$now = time();
$ts = strtotime("+1 hour");
$diff = $ts - $now;
if ($diff >= 3590 && $diff <= 3610) echo "ok";
"#,
    );
    assert_eq!(out, "ok");
}

/// Verifies `strtotime("-1 hour")` produces a timestamp roughly 3600 seconds behind now (±10s tolerance).
#[test]
fn test_strtotime_offset_minus_one_hour() {
    let out = compile_and_run(
        r#"<?php
$now = time();
$ts = strtotime("-1 hour");
$diff = $now - $ts;
if ($diff >= 3590 && $diff <= 3610) echo "ok";
"#,
    );
    assert_eq!(out, "ok");
}

/// Verifies `strtotime("1 hour ago")` produces a timestamp roughly 3600 seconds behind now (±10s tolerance).
#[test]
fn test_strtotime_offset_one_hour_ago() {
    let out = compile_and_run(
        r#"<?php
$now = time();
$ts = strtotime("1 hour ago");
$diff = $now - $ts;
if ($diff >= 3590 && $diff <= 3610) echo "ok";
"#,
    );
    assert_eq!(out, "ok");
}

/// Verifies `strtotime("a day ago")` produces a timestamp roughly 86400 seconds behind now
/// (allowing ±~1 hour for DST; range 82700–90100).
#[test]
fn test_strtotime_offset_article_day_ago() {
    let out = compile_and_run(
        r#"<?php
$now = time();
$ts = strtotime("a day ago");
$diff = $now - $ts;
if ($diff >= 86400 - 3700 && $diff <= 86400 + 3700) echo "ok";
"#,
    );
    assert_eq!(out, "ok");
}

/// Verifies `strtotime("an hour")` (article form) is equivalent to "+1 hour".
#[test]
fn test_strtotime_offset_article_an_hour() {
    let out = compile_and_run(
        r#"<?php
$now = time();
$ts = strtotime("an hour");
$diff = $ts - $now;
if ($diff >= 3590 && $diff <= 3610) echo "ok";
"#,
    );
    assert_eq!(out, "ok");
}

/// Verifies `strtotime("+30 seconds")` produces a timestamp roughly 30 seconds ahead of now (±2s tolerance).
#[test]
fn test_strtotime_offset_plus_30_seconds() {
    let out = compile_and_run(
        r#"<?php
$now = time();
$ts = strtotime("+30 seconds");
$diff = $ts - $now;
if ($diff >= 28 && $diff <= 32) echo "ok";
"#,
    );
    assert_eq!(out, "ok");
}

/// Verifies `strtotime("+2 weeks")` produces a timestamp roughly 14 days ahead (1209600 seconds ±1 day for DST).
#[test]
fn test_strtotime_offset_plus_two_weeks() {
    let out = compile_and_run(
        r#"<?php
$now = time();
$ts = strtotime("+2 weeks");
$diff = $ts - $now;
// 14 days = 1209600 seconds, allow ±1 day for DST
if ($diff >= 1209600 - 3700 && $diff <= 1209600 + 3700) echo "ok";
"#,
    );
    assert_eq!(out, "ok");
}

/// Verifies `strtotime` handles relative future offsets with multiple terms: "+1 day 2 hours"
/// (≈93600 seconds ±1 hour for DST).
#[test]
fn test_strtotime_offset_composite() {
    let out = compile_and_run(
        r#"<?php
$now = time();
$ts = strtotime("+1 day 2 hours");
$diff = $ts - $now;
// 1 day + 2 hours = 93600 seconds, allow ±1 hour for DST
if ($diff >= 93600 - 3700 && $diff <= 93600 + 3700) echo "ok";
"#,
    );
    assert_eq!(out, "ok");
}

/// Verifies `strtotime` allows ASCII whitespace (including newlines) between relative offset terms.
#[test]
fn test_strtotime_offset_allows_ascii_whitespace_between_terms() {
    let out = compile_and_run(
        r#"<?php
$now = time();
$ts = strtotime("+1 day
2 hours");
$diff = $ts - $now;
if ($diff >= 93600 - 3700 && $diff <= 93600 + 3700) echo "ok";
"#,
    );
    assert_eq!(out, "ok");
}

/// Verifies `strtotime("+1 month")` produces a timestamp roughly 28–31 days ahead (range 2400000–2678400 seconds).
#[test]
fn test_strtotime_offset_plus_one_month() {
    let out = compile_and_run(
        r#"<?php
$now = time();
$ts = strtotime("+1 month");
$diff = $ts - $now;
// 1 month ≈ 28..31 days = 2419200..2678400 seconds
if ($diff >= 2400000 && $diff <= 2700000) echo "ok";
"#,
    );
    assert_eq!(out, "ok");
}

/// Verifies `strtotime("+1 minute")` produces a timestamp roughly 60 seconds ahead (±2s tolerance).
#[test]
fn test_strtotime_offset_plus_one_minute() {
    let out = compile_and_run(
        r#"<?php
$now = time();
$ts = strtotime("+1 minute");
$diff = $ts - $now;
if ($diff >= 58 && $diff <= 62) echo "ok";
"#,
    );
    assert_eq!(out, "ok");
}

/// Verifies `strtotime("Monday")` (full weekday name) resolves to the upcoming Monday.
#[test]
fn test_strtotime_weekday_monday() {
    let out = compile_and_run(
        r#"<?php
$ts = strtotime("Monday");
echo date("D", $ts);
"#,
    );
    assert_eq!(out, "Mon");
}

/// Verifies `strtotime` weekday names are case-insensitive: "monday" resolves the same as "Monday".
#[test]
fn test_strtotime_weekday_lowercase() {
    let out = compile_and_run(
        r#"<?php
$ts = strtotime("monday");
echo date("D", $ts);
"#,
    );
    assert_eq!(out, "Mon");
}

/// Verifies `strtotime("Mon")` (3-letter abbreviation) resolves to the upcoming Monday.
#[test]
fn test_strtotime_weekday_abbrev() {
    let out = compile_and_run(
        r#"<?php
$ts = strtotime("Mon");
echo date("D", $ts);
"#,
    );
    assert_eq!(out, "Mon");
}

/// Verifies `strtotime` with the current weekday name as input (e.g. date("l")) resolves to today.
#[test]
fn test_strtotime_current_weekday_is_today() {
    let out = compile_and_run(
        r#"<?php
$weekday = date("l");
$ts = strtotime($weekday);
if (date("Y-m-d", $ts) == date("Y-m-d")) echo "ok";
"#,
    );
    assert_eq!(out, "ok");
}

/// Verifies `strtotime("next Friday")` resolves to the Friday in the future.
#[test]
fn test_strtotime_next_friday() {
    let out = compile_and_run(
        r#"<?php
$ts = strtotime("next Friday");
echo date("D", $ts);
"#,
    );
    assert_eq!(out, "Fri");
}

/// Verifies `strtotime("last Sunday")` resolves to the Sunday in the past.
#[test]
fn test_strtotime_last_sunday() {
    let out = compile_and_run(
        r#"<?php
$ts = strtotime("last Sunday");
echo date("D", $ts);
"#,
    );
    assert_eq!(out, "Sun");
}

/// Verifies `strtotime("this Wednesday")` resolves to the Wednesday of the current week.
#[test]
fn test_strtotime_this_wednesday() {
    let out = compile_and_run(
        r#"<?php
$ts = strtotime("this Wednesday");
echo date("D", $ts);
"#,
    );
    assert_eq!(out, "Wed");
}

/// Verifies `strtotime("3 days ago")` produces a timestamp roughly 3 days behind now
/// (259200 seconds ±1 hour for DST).
#[test]
fn test_strtotime_offset_3_days_ago() {
    let out = compile_and_run(
        r#"<?php
$now = time();
$ts = strtotime("3 days ago");
$diff = $now - $ts;
// 3 days = 259200, allow ±1 hour for DST
if ($diff >= 259200 - 3700 && $diff <= 259200 + 3700) echo "ok";
"#,
    );
    assert_eq!(out, "ok");
}

/// Verifies `date()` with no timestamp argument uses the current time and returns a year >= 2024.
#[test]
fn test_date_current_time() {
    // date() with no timestamp should use current time
    let out = compile_and_run(
        "<?php $y = date(\"Y\"); $val = intval($y); if ($val >= 2024) { echo \"ok\"; }",
    );
    assert_eq!(out, "ok");
}

/// Verifies `date("l", timestamp)` returns the full weekday name (Monday…Sunday) for a known UTC timestamp.
#[test]
fn test_date_full_day_name() {
    let out = compile_and_run("<?php echo date(\"l\", 1700000000);");
    let valid_days = [
        "Monday",
        "Tuesday",
        "Wednesday",
        "Thursday",
        "Friday",
        "Saturday",
        "Sunday",
    ];
    assert!(valid_days.contains(&out.as_str()), "Got: {}", out);
}

/// Verifies `date("F", timestamp)` returns the full month name ("November") for a known UTC timestamp.
#[test]
fn test_date_full_month_name() {
    let out = compile_and_run("<?php echo date(\"F\", 1700000000);");
    assert_eq!(out, "November");
}

/// Regression test for GitHub issue #9: `date("Y-m-d", 0)` must format Unix epoch
/// (1970-01-01 00:00:00 UTC), not return the current date.
#[test]
fn test_date_epoch_zero_timestamp() {
    // Regression test for GitHub issue #9: date("Y-m-d", 0) should format Unix epoch,
    // not return the current date. Timestamp 0 = 1970-01-01 00:00:00 UTC.
    let out = compile_and_run("<?php echo date(\"Y\", 0);");
    assert_eq!(out, "1970");
}

// --- JSON functions ---

/// Verifies `json_encode(int)` emits the decimal string representation without quotes.
#[test]
fn test_json_encode_int() {
    let out = compile_and_run("<?php echo json_encode(42);");
    assert_eq!(out, "42");
}

/// Verifies `json_encode("string")` emits a double-quoted JSON string.
#[test]
fn test_json_encode_string() {
    let out = compile_and_run(r#"<?php echo json_encode("hello");"#);
    assert_eq!(out, r#""hello""#);
}

/// Verifies `json_encode` escapes newline characters as `\n` in the output string.
#[test]
fn test_json_encode_string_with_escaping() {
    let out = compile_and_run("<?php echo json_encode(\"hello\\nworld\");");
    assert_eq!(out, r#""hello\nworld""#);
}

/// Verifies `json_encode` escapes interior double-quotes as `\"` in the output string.
#[test]
fn test_json_encode_string_with_quotes() {
    let out = compile_and_run(r#"<?php echo json_encode("say \"hi\"");"#);
    assert_eq!(out, r#""say \"hi\"""#);
}

/// Verifies `json_encode(true)` emits the JSON literal `true`.
#[test]
fn test_json_encode_bool_true() {
    let out = compile_and_run("<?php echo json_encode(true);");
    assert_eq!(out, "true");
}

/// Verifies `json_encode(false)` emits the JSON literal `false`.
#[test]
fn test_json_encode_bool_false() {
    let out = compile_and_run("<?php echo json_encode(false);");
    assert_eq!(out, "false");
}

/// Verifies `json_encode(null)` emits the JSON literal `null`.
#[test]
fn test_json_encode_null() {
    let out = compile_and_run("<?php echo json_encode(null);");
    assert_eq!(out, "null");
}

/// Verifies `json_encode([1, 2, 3])` emits a compact JSON array without whitespace.
#[test]
fn test_json_encode_int_array() {
    let out = compile_and_run("<?php echo json_encode([1, 2, 3]);");
    assert_eq!(out, "[1,2,3]");
}

/// Verifies `json_encode(["a", "b", "c"])` emits a compact JSON array of strings.
#[test]
fn test_json_encode_string_array() {
    let out = compile_and_run(r#"<?php echo json_encode(["a", "b", "c"]);"#);
    assert_eq!(out, r#"["a","b","c"]"#);
}

/// Verifies `json_encode` escapes `\n`, `\"`, and `\\` correctly inside string array elements.
#[test]
fn test_json_encode_string_array_with_escaping() {
    let out = compile_and_run("<?php echo json_encode([\"a\\n\", \"b\\\"\", \"c\\\\\"]);");
    assert_eq!(out, "[\"a\\n\",\"b\\\"\",\"c\\\\\"]");
}

/// Verifies `json_encode([42])` (single-element array) emits `[42]`.
#[test]
fn test_json_encode_single_element_array() {
    let out = compile_and_run("<?php $arr = [42]; echo json_encode($arr);");
    assert_eq!(out, "[42]");
}

/// Verifies `json_encode(["name" => "Alice", "age" => "30"])` emits a compact JSON object.
#[test]
fn test_json_encode_assoc() {
    let out = compile_and_run(r#"<?php echo json_encode(["name" => "Alice", "age" => "30"]);"#);
    assert_eq!(out, r#"{"name":"Alice","age":"30"}"#, "Got: {}", out);
}

/// Verifies `json_encode([1 => "one", "02" => "two"])` converts integer and string keys to JSON
/// property names and emits a compact object.
#[test]
fn test_json_encode_assoc_integer_keys() {
    let out = compile_and_run(r#"<?php echo json_encode([1 => "one", "02" => "two"]);"#);
    assert_eq!(out, r#"{"1":"one","02":"two"}"#);
}

/// Verifies `json_encode` with a mixed-value associative array (int, string, bool, null) emits
/// the correct JSON property names and values for each type.
#[test]
fn test_json_encode_assoc_mixed_values() {
    let out = compile_and_run(
        r#"<?php echo json_encode(["id" => 7, "name" => "Alice", "ok" => true, "note" => null]);"#,
    );
    assert_eq!(out, r#"{"id":7,"name":"Alice","ok":true,"note":null}"#);
}

/// Verifies `json_encode` correctly handles nested non-string-indexed arrays containing floats,
/// booleans, and empty objects; each type serializes to its JSON equivalent.
#[test]
fn test_json_encode_assoc_nested_nonstring_indexed_arrays() {
    let out = compile_and_run(
        r#"<?php
class Box {}
echo json_encode([
    "floats" => [1.5, 2.25],
    "bools" => [true, false],
    "objects" => [new Box()],
]);
"#,
    );
    assert_eq!(
        out,
        r#"{"floats":[1.5,2.25],"bools":[true,false],"objects":[{}]}"#
    );
}

/// Verifies `json_encode(3.14)` emits a JSON number; output starts with "3.14".
#[test]
fn test_json_encode_float() {
    let out = compile_and_run("<?php echo json_encode(3.14);");
    assert!(out.starts_with("3.14"), "Got: {}", out);
}

/// Verifies `json_last_error()` returns 0 (JSON_ERROR_NONE) after a successful encode with no errors.
#[test]
fn test_json_last_error() {
    let out = compile_and_run("<?php echo json_last_error();");
    assert_eq!(out, "0");
}

/// Verifies `json_decode("\"hello\"")` decodes a JSON string to the plain PHP string "hello".
#[test]
fn test_json_decode_string() {
    let out = compile_and_run(r#"<?php echo json_decode("\"hello\"");"#);
    assert_eq!(out, "hello");
}

/// Verifies `json_decode("42")` decodes a JSON number to the PHP integer 42 (echoed as "42").
#[test]
fn test_json_decode_number() {
    let out = compile_and_run(r#"<?php echo json_decode("42");"#);
    assert_eq!(out, "42");
}

/// Verifies `json_decode("\"hello\\nworld\"")` correctly unescapes the newline character;
/// `strlen` returns 11 (5 + 1 + 5).
#[test]
fn test_json_decode_escaped() {
    let out = compile_and_run(r#"<?php $s = json_decode("\"hello\\nworld\""); echo strlen($s);"#);
    assert_eq!(out, "11"); // "hello" + newline + "world" = 11 chars
}

/// Verifies `json_decode("\"a\\\"b\\\\c\"")` correctly unescapes interior quote and backslash
/// to produce `a"b\c`.
#[test]
fn test_json_decode_escaped_quote_and_backslash() {
    let out = compile_and_run(r#"<?php echo json_decode("\"a\\\"b\\\\c\"");"#);
    assert_eq!(out, "a\"b\\c");
}

/// Verifies `json_decode` trims leading and trailing whitespace from the JSON input string.
#[test]
fn test_json_decode_trimmed_string() {
    let out = compile_and_run(r#"<?php echo json_decode("   \"hello\"   ");"#);
    assert_eq!(out, "hello");
}

/// Verifies `json_decode(" true ")` decodes the JSON literal `true`; PHP's bool→string coercion
/// causes `echo true` to print "1".
#[test]
fn test_json_decode_true_literal() {
    // PHP-faithful: json_decode("true") returns Mixed(bool=true), and
    // `echo true` prints "1" (PHP's bool→string coercion rule).
    let out = compile_and_run(r#"<?php echo json_decode(" true ");"#);
    assert_eq!(out, "1");
}

/// Verifies `json_decode(" null ")` decodes the JSON literal `null`; PHP's `echo null` produces
/// the empty string.
#[test]
fn test_json_decode_null_literal() {
    // PHP-faithful: json_decode("null") returns Mixed(null), and `echo null`
    // prints nothing (the empty string).
    let out = compile_and_run(r#"<?php echo json_decode("\nnull\t");"#);
    assert_eq!(out, "");
}

/// Verifies a JSON array round-trip: `json_decode(" [1, 2, 3] ")` returns a structural Mixed(array)
/// and `json_encode` produces the compact form `[1,2,3]`.
#[test]
fn test_json_decode_array_round_trip() {
    // json_decode now returns a structural Mixed(array) for non-empty
    // arrays; round-trip through json_encode produces the canonical
    // compact form (whitespace dropped, scalars re-encoded).
    let out = compile_and_run(r#"<?php echo json_encode(json_decode(" [1, 2, 3] "));"#);
    assert_eq!(out, "[1,2,3]");
}

/// Verifies a JSON object round-trip: `json_decode(" {\"a\": 1} ")` returns a structural Mixed(assoc)
/// and `json_encode` produces the compact form `{"a":1}`.
#[test]
fn test_json_decode_assoc_round_trip() {
    // json_decode now returns a structural Mixed(assoc) for non-empty
    // objects too; round-trip through json_encode produces the canonical
    // compact form.
    let out = compile_and_run(r#"<?php echo json_encode(json_decode(" {\"a\": 1} "));"#);
    assert_eq!(out, r#"{"a":1}"#);
}

/// Verifies `json_decode` decodes a JSON string containing an escaped solidus (`\/`) to `/`.
#[test]
fn test_json_decode_escaped_solidus() {
    let out = compile_and_run(r#"<?php echo json_decode("\"https:\\/\\/example.com\"");"#);
    assert_eq!(out, "https://example.com");
}

/// Verifies `json_decode("\"caf\u00e9\"")` decodes a BMP Unicode escape (Latin-1 Supplement) to the string "café".
#[test]
fn test_json_decode_unicode_bmp_latin1() {
    let out = compile_and_run(r#"<?php echo json_decode("\"caf\u00e9\"");"#);
    assert_eq!(out, "café");
}

/// Verifies `json_decode("\"\u4f60\u597d\"")` decodes two BMP Unicode escapes (CJK Unified Ideographs) to "你好".
#[test]
fn test_json_decode_unicode_bmp_multibyte() {
    let out = compile_and_run(r#"<?php echo json_decode("\"\u4f60\u597d\"");"#);
    assert_eq!(out, "你好");
}

/// Verifies `json_decode` decodes a JSON string containing a surrogate pair (`\ud83d\ude00`, emoji U+1F600)
/// to its UTF-8 representation (4 bytes); `strlen` returns 4.
#[test]
fn test_json_decode_unicode_surrogate_pair() {
    let out = compile_and_run(r#"<?php $s = json_decode("\"\ud83d\ude00\""); echo strlen($s);"#);
    assert_eq!(out, "4");
}

// --- Regex functions ---

/// Verifies `preg_match("/hello/", "hello world")` returns 1 (match found).
#[test]
fn test_preg_match_simple() {
    let out = compile_and_run(r#"<?php echo preg_match("/hello/", "hello world");"#);
    assert_eq!(out, "1");
}

/// Verifies literal `call_user_func()` dispatch to `preg_match()` includes regex runtime helpers.
#[test]
fn test_preg_match_call_user_func_literal() {
    let out = compile_and_run(r#"<?php echo call_user_func("preg_match", "/a/", "cat");"#);
    assert_eq!(out, "1");
}

/// Verifies first-class `preg_replace_callback()` references include regex runtime helpers.
#[test]
fn test_preg_replace_callback_first_class_callable_runtime() {
    let out = compile_and_run(
        r#"<?php
$cb = preg_replace_callback(...);
echo $cb("/[0-9]+/", function($m): string { return "X"; }, "a12b");
"#,
    );
    assert_eq!(out, "aXb");
}

/// Verifies first-class `preg_replace_callback()` still types callback matches as arrays.
#[test]
fn test_preg_replace_callback_first_class_callable_match_array_context() {
    let out = compile_and_run(
        r#"<?php
$cb = preg_replace_callback(...);
echo $cb("/a/", function($m): string { return strtoupper($m[0]); }, "cat");
"#,
    );
    assert_eq!(out, "cAt");
}

/// Verifies `preg_match("/xyz/", "hello world")` returns 0 (no match).
#[test]
fn test_preg_match_no_match() {
    let out = compile_and_run(r#"<?php echo preg_match("/xyz/", "hello world");"#);
    assert_eq!(out, "0");
}

/// Verifies `preg_match` with the `i` modifier performs case-insensitive matching.
#[test]
fn test_preg_match_case_insensitive() {
    let out = compile_and_run(r#"<?php echo preg_match("/HELLO/i", "hello world");"#);
    assert_eq!(out, "1");
}

/// Verifies `preg_match("/[0-9]+/", "abc123def")` finds a digit sequence in the subject.
#[test]
fn test_preg_match_pattern() {
    let out = compile_and_run(r#"<?php echo preg_match("/[0-9]+/", "abc123def");"#);
    assert_eq!(out, "1");
}

/// Verifies PCRE positive lookahead works through the PCRE2-backed regex runtime.
#[test]
fn test_preg_match_pcre_positive_lookahead() {
    let out = compile_and_run(r#"<?php echo preg_match("/foo(?=bar)/", "foobar");"#);
    assert_eq!(out, "1");
}

/// Verifies PCRE positive lookbehind works through the PCRE2-backed regex runtime.
#[test]
fn test_preg_match_pcre_positive_lookbehind() {
    let out = compile_and_run(r#"<?php echo preg_match("/(?<=foo)bar/", "foobar");"#);
    assert_eq!(out, "1");
}

/// Verifies `preg_match()` writes the full match and capture groups into `$matches`.
#[test]
fn test_preg_match_populates_matches_array() {
    let out = compile_and_run(
        r#"<?php
$ok = preg_match("/(a)(b)/", "zab", $matches);
echo $ok . "|" . count($matches) . "|" . $matches[0] . "," . $matches[1] . "," . $matches[2];
"#,
    );
    assert_eq!(out, "1|3|ab,a,b");
}

/// Verifies `preg_match()` sizes `$matches` from the compiled capture count, not a fixed window.
#[test]
fn test_preg_match_populates_matches_beyond_ninety_nine() {
    let out = compile_and_run(
        r#"<?php
$pattern = "/";
$subject = "";
$i = 0;
while ($i < 105) {
    $pattern = $pattern . "(a)";
    $subject = $subject . "a";
    $i = $i + 1;
}
$pattern = $pattern . "/";
preg_match($pattern, $subject, $matches);
echo count($matches) . "|" . $matches[100] . $matches[105];
"#,
    );
    assert_eq!(out, "106|aa");
}

/// Verifies `preg_match()` replaces an existing matches variable with an empty array on no-match.
#[test]
fn test_preg_match_no_match_clears_matches_array() {
    let out = compile_and_run(
        r#"<?php
$matches = ["old"];
$ok = preg_match("/x/", "abc", $matches);
echo $ok . "|" . count($matches);
"#,
    );
    assert_eq!(out, "0|0");
}

/// Verifies unmatched optional captures before later captures materialize as empty strings.
#[test]
fn test_preg_match_unmatched_interior_capture_is_empty() {
    let out = compile_and_run(
        r#"<?php
preg_match("/(a)?(b)/", "b", $matches);
echo count($matches) . "|" . $matches[0] . "|" . $matches[1] . "|" . $matches[2];
"#,
    );
    assert_eq!(out, "3|b||b");
}

/// Verifies named arguments can provide the optional by-reference `$matches` output variable.
#[test]
fn test_preg_match_named_matches_argument() {
    let out = compile_and_run(
        r#"<?php
preg_match(pattern: "/([0-9]+)/", subject: "id=42", matches: $matches);
echo $matches[1];
"#,
    );
    assert_eq!(out, "42");
}

/// Verifies a `$matches` variable created by `preg_match()` is visible after an `if` condition.
#[test]
fn test_preg_match_matches_array_visible_after_condition() {
    let out = compile_and_run(
        r#"<?php
if (preg_match("/([A-Z]+)/", "abcXYZ", $matches)) {
    echo $matches[1];
}
"#,
    );
    assert_eq!(out, "XYZ");
}

/// Verifies `preg_match("/[0-9]+/", "abcdef")` returns 0 when no digits are present.
#[test]
fn test_preg_match_no_digits() {
    let out = compile_and_run(r#"<?php echo preg_match("/[0-9]+/", "abcdef");"#);
    assert_eq!(out, "0");
}

/// Verifies `preg_match` with the Unicode property escape `\p{L}+` matches a run of letters
/// including non-ASCII characters (Japanese kana).
#[test]
fn test_preg_match_unicode_property_letter() {
    let out = compile_and_run(r#"<?php echo preg_match("/\p{L}+/u", "日本語123");"#);
    assert_eq!(out, "1");
}

/// Verifies `preg_match` with `\p{L}+` at the start and end anchors rejects a string that
/// contains digits after the letter run ("日本語123").
#[test]
fn test_preg_match_unicode_property_letter_rejects_digit_suffix() {
    let out = compile_and_run(r#"<?php echo preg_match("/^\p{L}+$/u", "日本語123");"#);
    assert_eq!(out, "0");
}

/// Verifies `\p{Lu}` (uppercase letter) followed by `\p{Ll}+` (lowercase letter run) matches
/// a capitalized word like "Hello" via the case-insensitive modifier aliases.
#[test]
fn test_preg_match_unicode_property_case_aliases() {
    let out = compile_and_run(r#"<?php echo preg_match("/^\p{Lu}\p{Ll}+$/u", "Hello");"#);
    assert_eq!(out, "1");
}

/// Verifies `\P{N}+` (negated numeric property) matches a run of non-digit characters like "abc".
#[test]
fn test_preg_match_negated_unicode_property() {
    let out = compile_and_run(r#"<?php echo preg_match("/^\P{N}+$/u", "abc");"#);
    assert_eq!(out, "1");
}

/// Verifies `preg_match_all` returns the count of non-overlapping matches (3 digits in "a1b2c3").
#[test]
fn test_preg_match_all_count() {
    let out = compile_and_run(r#"<?php echo preg_match_all("/[0-9]+/", "a1b2c3");"#);
    assert_eq!(out, "3");
}

/// Verifies `preg_match_all` returns 0 when the pattern has no matches in the subject.
#[test]
fn test_preg_match_all_no_matches() {
    let out = compile_and_run(r#"<?php echo preg_match_all("/[0-9]+/", "abcdef");"#);
    assert_eq!(out, "0");
}

/// Verifies `preg_replace` substitutes the first matching occurrence of a literal pattern.
#[test]
fn test_preg_replace_simple() {
    let out = compile_and_run(r#"<?php echo preg_replace("/world/", "PHP", "hello world");"#);
    assert_eq!(out, "hello PHP");
}

/// Verifies `preg_replace` substitutes all non-overlapping matches of a digit pattern.
#[test]
fn test_preg_replace_pattern() {
    let out = compile_and_run(r#"<?php echo preg_replace("/[0-9]+/", "X", "a1b2c3");"#);
    assert_eq!(out, "aXbXcX");
}

/// Verifies PCRE lazy quantifiers keep their non-greedy behavior through PCRE2.
#[test]
fn test_preg_replace_pcre_lazy_quantifier() {
    let out = compile_and_run(r#"<?php echo preg_replace("/a+?/", "X", "aaa");"#);
    assert_eq!(out, "XXX");
}

/// Verifies `preg_replace` with Unicode property escape `\p{N}+` replaces all digit runs in a string.
#[test]
fn test_preg_replace_unicode_property_number() {
    let out = compile_and_run(r#"<?php echo preg_replace("/\p{N}+/u", "X", "abc123def456");"#);
    assert_eq!(out, "abcXdefX");
}

/// Verifies `preg_replace` returns the subject unchanged when the pattern has no matches.
#[test]
fn test_preg_replace_no_match() {
    let out = compile_and_run(r#"<?php echo preg_replace("/xyz/", "ABC", "hello world");"#);
    assert_eq!(out, "hello world");
}

/// Verifies `preg_replace_callback` invokes the closure for each match and the callback return
/// value replaces the matched text; captures are accessible via `$matches[0]`.
#[test]
fn test_preg_replace_callback_matches_array() {
    let out = compile_and_run(
        r#"<?php
$result = preg_replace_callback(
    "/(\d+)/",
    function($matches) {
        return "[" . $matches[0] . "]";
    },
    "price: 123 and 456"
);
echo $result;
"#,
    );
    assert_eq!(out, "price: [123] and [456]");
}

/// Verifies `preg_replace_callback` exposes both complete match `$matches[0]` and numbered
/// capture groups `$matches[1]`, `$matches[2]` to the closure.
#[test]
fn test_preg_replace_callback_capture_groups() {
    let out = compile_and_run(
        r#"<?php
echo preg_replace_callback(
    "/([a-z]+)-([0-9]+)/",
    function($matches) {
        return $matches[1] . ":" . $matches[2];
    },
    "id-42 and item-7"
);
"#,
    );
    assert_eq!(out, "id:42 and item:7");
}

/// Verifies `preg_replace_callback` materializes captures beyond `$matches[9]`.
#[test]
fn test_preg_replace_callback_capture_groups_beyond_nine() {
    let out = compile_and_run(
        r#"<?php
echo preg_replace_callback(
    "/(a)(b)(c)(d)(e)(f)(g)(h)(i)(j)(k)(l)/",
    function($matches) {
        return $matches[10] . $matches[11] . $matches[12];
    },
    "abcdefghijkl"
);
"#,
    );
    assert_eq!(out, "jkl");
}

/// Verifies `preg_replace_callback` materializes every compiled capture group beyond the
/// old fixed 99-capture runtime window.
#[test]
fn test_preg_replace_callback_capture_groups_beyond_ninety_nine() {
    let out = compile_and_run(
        r#"<?php
$pattern = "/";
$subject = "";
for ($i = 1; $i <= 105; $i = $i + 1) {
    $pattern = $pattern . "(.)";
    $subject = $subject . ($i === 105 ? "z" : "a");
}
$pattern = $pattern . "/";
echo preg_replace_callback(
    $pattern,
    function($matches) {
        return count($matches) . ":" . $matches[105];
    },
    $subject
);
"#,
    );
    assert_eq!(out, "106:z");
}

/// Verifies `preg_replace_callback` keeps interior unmatched captures as empty strings
/// while omitting the trailing unmatched capture group from the callback array.
#[test]
fn test_preg_replace_callback_unmatched_interior_capture_is_empty() {
    let out = compile_and_run(
        r#"<?php
echo preg_replace_callback(
    "/(a)?(b)(c)?/",
    function($matches) {
        return count($matches) . ":" . $matches[1] . ":" . $matches[2];
    },
    "b"
);
"#,
    );
    assert_eq!(out, "3::b");
}

/// Verifies `preg_replace_callback` closure captures a by-value `use` variable and the captured
/// value is available inside the callback for constructing the replacement string.
#[test]
fn test_preg_replace_callback_closure_capture_by_value() {
    let out = compile_and_run(
        r#"<?php
$prefix = "n:";
echo preg_replace_callback(
    "/([0-9]+)/",
    function($matches) use ($prefix) {
        return $prefix . $matches[1];
    },
    "a1 b22"
);
"#,
    );
    assert_eq!(out, "an:1 bn:22");
}

/// Verifies `preg_replace_callback` closure captures a variable by reference (`use &$count`);
/// mutations inside the callback are visible to the caller after the call.
#[test]
fn test_preg_replace_callback_closure_capture_by_ref() {
    let out = compile_and_run(
        r#"<?php
$count = 0;
$result = preg_replace_callback(
    "/[0-9]+/",
    function($matches) use (&$count) {
        $count = $count + 1;
        return "[" . $count . ":" . $matches[0] . "]";
    },
    "a1 b22"
);
echo $result . "|" . $count;
"#,
    );
    assert_eq!(out, "a[1:1] b[2:22]|2");
}

/// Verifies a captured closure variable used by `preg_replace_callback()` reads captures
/// from the stored descriptor rather than from the reassigned source local.
#[test]
fn test_preg_replace_callback_closure_variable_uses_descriptor_capture_after_reassign() {
    let out = compile_and_run(
        r#"<?php
$prefix = "old:";
$cb = function(array $matches) use ($prefix): string {
    return $prefix;
};
$prefix = "new:";
echo preg_replace_callback("/[0-9]+/", $cb, "a1 b22");
"#,
    );
    assert_eq!(out, "aold: bold:");
}

/// Verifies a method first-class callable passed as a `callable` parameter to
/// `preg_replace_callback()` keeps the receiver captured in the descriptor.
#[test]
fn test_preg_replace_callback_method_parameter_uses_descriptor_receiver() {
    let out = compile_and_run(
        r#"<?php
class RegexFormatter {
    public function __construct(private string $prefix) {}

    public function replace(array $matches): string {
        return $this->prefix;
    }
}

function run_regex(callable $cb): void {
    echo preg_replace_callback("/[A-Z]/", $cb, "AB");
}

run_regex((new RegexFormatter("descriptor:"))->replace(...));
"#,
    );
    assert_eq!(out, "descriptor:descriptor:");
}

/// Verifies callable-array regex callbacks route through descriptor environments.
#[test]
fn test_preg_replace_callback_callable_array_variable_preserves_receiver() {
    let out = compile_and_run(
        r#"<?php
class RegexArrayFormatter {
    public string $prefix = "";

    public function replace(array $matches): string {
        return $this->prefix;
    }
}

$first = new RegexArrayFormatter();
$first->prefix = "first:";
$second = new RegexArrayFormatter();
$second->prefix = "second:";
$callback = [$first, "replace"];
$first = $second;
echo preg_replace_callback("/[A-Z]/", $callback, "AB");
"#,
    );
    assert_eq!(out, "first:first:");
}

/// Verifies runtime-selected instance callable arrays route regex callbacks through descriptors.
#[test]
fn test_preg_replace_callback_runtime_selected_instance_callable_array() {
    let out = compile_and_run(
        r#"<?php
class RuntimeRegexArrayFormatter {
    public string $prefix = "";

    public function replace(array $matches): string {
        return $this->prefix . count($matches);
    }
}

$first = new RuntimeRegexArrayFormatter();
$first->prefix = "I:";
$second = new RuntimeRegexArrayFormatter();
$second->prefix = "bad:";
$method = "replace";
$callback = [$first, $method];
$first = $second;
echo preg_replace_callback("/[A-Z]/", $callback, "AB");
"#,
    );
    assert_eq!(out, "I:1I:1");
}

/// Verifies runtime-selected static callable arrays route regex callbacks through descriptors.
#[test]
fn test_preg_replace_callback_runtime_selected_static_callable_array() {
    let out = compile_and_run(
        r#"<?php
class RuntimeRegexStaticFormatter {
    public static function replace(array $matches): string {
        return "S:" . count($matches);
    }
}

$class = "RuntimeRegexStaticFormatter";
$method = "replace";
$callback = [$class, $method];
echo preg_replace_callback("/[A-Z]/", $callback, "AB");
"#,
    );
    assert_eq!(out, "S:1S:1");
}

/// Verifies runtime string user callbacks route `preg_replace_callback()` through descriptors.
#[test]
fn test_preg_replace_callback_runtime_string_user_callback() {
    let out = compile_and_run(
        r#"<?php
function runtime_regex_replace(array $matches): string {
    return "U" . count($matches);
}

$callback = "runtime_regex_replace";
echo preg_replace_callback("/[A-Z]/", $callback, "AB");
"#,
    );
    assert_eq!(out, "U1U1");
}

/// Verifies runtime string static-method callbacks route regex replacements through descriptors.
#[test]
fn test_preg_replace_callback_runtime_string_static_method_callback() {
    let out = compile_and_run(
        r#"<?php
class RuntimeStringRegexFormatter {
    public static function replace(array $matches): string {
        return "S" . count($matches);
    }
}

$callback = "RuntimeStringRegexFormatter::replace";
echo preg_replace_callback("/[A-Z]/", $callback, "AB");
"#,
    );
    assert_eq!(out, "S1S1");
}

/// Verifies a branch-selected first-class callable keeps the selected descriptor
/// environment when passed directly to `preg_replace_callback()`.
#[test]
fn test_preg_replace_callback_branch_selected_method_descriptor() {
    let out = compile_and_run(
        r#"<?php
class RegexFormatter {
    public function __construct(private string $prefix) {}

    public function replace(array $matches): string {
        return $this->prefix;
    }
}

$left = new RegexFormatter("left:");
$right = new RegexFormatter("right:");
$useRight = true;
echo preg_replace_callback("/[A-Z]/", $useRight ? $right->replace(...) : $left->replace(...), "AB");
"#,
    );
    assert_eq!(out, "right:right:");
}

/// Verifies `preg_split` splits a string on a comma delimiter and returns an indexed array
/// with all 3 parts.
#[test]
fn test_preg_split_simple() {
    let out = compile_and_run(
        r#"<?php
$parts = preg_split("/,/", "a,b,c");
echo count($parts) . "|" . $parts[0] . "|" . $parts[1] . "|" . $parts[2];
"#,
    );
    assert_eq!(out, "3|a|b|c");
}

/// Verifies `preg_split` with a regex pattern `"/[ ]+/"` splits on one or more spaces
/// and discards empty trailing parts.
#[test]
fn test_preg_split_whitespace() {
    let out = compile_and_run(
        r#"<?php
$parts = preg_split("/[ ]+/", "hello   world");
echo count($parts) . "|" . $parts[0] . "|" . $parts[1];
"#,
    );
    assert_eq!(out, "2|hello|world");
}

/// Verifies `preg_split` applies limit, delimiter capture, and offset capture flags.
#[test]
fn test_preg_split_limit_delimiter_and_offset_capture() {
    let out = compile_and_run(
        r#"<?php
$parts = preg_split("/([,])/", "a,b,c", 2, PREG_SPLIT_DELIM_CAPTURE | PREG_SPLIT_OFFSET_CAPTURE);
echo count($parts) . "|";
foreach ($parts as $part) {
    echo $part[0] . "@" . $part[1] . ";";
}
"#,
    );
    assert_eq!(out, "3|a@0;,@1;b,c@2;");
}

/// Verifies `preg_split` delimiter capture materializes capture groups beyond the old
/// fixed 99-capture runtime window.
#[test]
fn test_preg_split_delimiter_capture_beyond_ninety_nine() {
    let out = compile_and_run(
        r#"<?php
$pattern = "/";
$subject = "";
for ($i = 1; $i <= 105; $i = $i + 1) {
    $pattern = $pattern . "(.)";
    $subject = $subject . ($i === 105 ? "z" : "a");
}
$pattern = $pattern . "/";
$parts = preg_split($pattern, $subject, -1, PREG_SPLIT_NO_EMPTY | PREG_SPLIT_DELIM_CAPTURE);
echo count($parts);
echo ":";
echo $parts[104];
"#,
    );
    assert_eq!(out, "105:z");
}

/// Verifies `preg_replace` with the `i` modifier performs case-insensitive substitution.
#[test]
fn test_preg_replace_case_insensitive() {
    let out = compile_and_run(r#"<?php echo preg_replace("/WORLD/i", "PHP", "hello World");"#);
    assert_eq!(out, "hello PHP");
}

/// Verifies `preg_replace` interprets `$1`, `$2` backreferences in the replacement string
/// using the matched capture groups.
#[test]
fn test_preg_replace_dollar_backreferences() {
    let out = compile_and_run(
        r#"<?php echo preg_replace("/([a-z]+) ([a-z]+)/", '$2, $1', "hello world");"#,
    );
    assert_eq!(out, "world, hello");
}

/// Verifies `preg_replace` interprets `\1`, `\2` backreferences in the replacement string
/// using the matched capture groups.
#[test]
fn test_preg_replace_backslash_backreferences() {
    let out = compile_and_run(r#"<?php echo preg_replace("/([0-9]+)-([0-9]+)/", "\\2/\\1", "12-34");"#);
    assert_eq!(out, "34/12");
}

/// Verifies `preg_replace` expands two-digit replacement backreferences and leaves a
/// third digit literal, matching PHP's `$99` / `$990` parsing.
#[test]
fn test_preg_replace_two_digit_backreferences() {
    let out = compile_and_run(
        r#"<?php
$pattern = "/";
$subject = "";
for ($i = 1; $i <= 99; $i = $i + 1) {
    $pattern = $pattern . "(.)";
    $subject = $subject . ($i === 99 ? "z" : "a");
}
$pattern = $pattern . "/";
echo preg_replace($pattern, '$99-$990-$100', $subject);
"#,
    );
    assert_eq!(out, "z-z0-a0");
}

/// Verifies `preg_replace` with a pattern containing an optional capture group renders an
/// unmatched group as an empty string in the replacement (e.g. "(a)(b)?" with "a" → "[a][]").
#[test]
fn test_preg_replace_unmatched_capture_backreference_is_empty() {
    let out = compile_and_run(r#"<?php echo preg_replace("/(a)(b)?/", '[$1][$2]', "a");"#);
    assert_eq!(out, "[a][]");
}
// is_callable() — compile-time decisions for string literals (catalog
// lookup) and Callable-typed values (closures + first-class callables).

/// Verifies `is_callable("json_encode")` returns true for a known built-in function (lowercase).
#[test]
fn test_is_callable_known_builtin_returns_true() {
    let out = compile_and_run(r#"<?php echo is_callable("json_encode") ? "y" : "n";"#);
    assert_eq!(out, "y");
}

/// Verifies `is_callable("nope_xyz_no_such_fn")` returns false for a string that does not
/// correspond to any declared function.
#[test]
fn test_is_callable_unknown_string_returns_false() {
    let out = compile_and_run(r#"<?php echo is_callable("nope_xyz_no_such_fn") ? "y" : "n";"#);
    assert_eq!(out, "n");
}

/// Verifies `is_callable` performs case-insensitive lookup for built-in function names.
#[test]
fn test_is_callable_case_insensitive_builtin() {
    let out = compile_and_run(r#"<?php echo is_callable("Json_Encode") ? "y" : "n";"#);
    assert_eq!(out, "y");
}

/// Verifies `is_callable("JSON_DECODE")` (all uppercase) still returns true for the built-in
/// `json_decode` function, confirming case-insensitive builtin resolution.
#[test]
fn test_is_callable_uppercase_builtin() {
    let out = compile_and_run(r#"<?php echo is_callable("JSON_DECODE") ? "y" : "n";"#);
    assert_eq!(out, "y");
}

/// Verifies `is_callable` recognises a user-defined function by its string name.
#[test]
fn test_is_callable_user_function_returns_true() {
    let out = compile_and_run(
        r#"<?php
            function my_user_fn() { return 1; }
            echo is_callable("my_user_fn") ? "y" : "n";
        "#,
    );
    assert_eq!(out, "y");
}

/// Verifies `is_callable` returns true when passed a closure (anonymous function object).
#[test]
fn test_is_callable_closure_returns_true() {
    let out = compile_and_run(
        r#"<?php
            $f = function() { return 42; };
            echo is_callable($f) ? "y" : "n";
        "#,
    );
    assert_eq!(out, "y");
}

/// Verifies `is_callable` returns true for a first-class callable created from a built-in function
/// (e.g. `json_encode(...)`).
#[test]
fn test_is_callable_first_class_callable_returns_true() {
    let out = compile_and_run(
        r#"<?php
            $f = json_encode(...);
            echo is_callable($f) ? "y" : "n";
        "#,
    );
    assert_eq!(out, "y");
}

/// Verifies `is_callable` returns false for a plain integer.
#[test]
fn test_is_callable_int_returns_false() {
    let out = compile_and_run(r#"<?php echo is_callable(42) ? "y" : "n";"#);
    assert_eq!(out, "n");
}

/// Verifies `is_callable` returns false for a boolean.
#[test]
fn test_is_callable_bool_returns_false() {
    let out = compile_and_run(r#"<?php echo is_callable(true) ? "y" : "n";"#);
    assert_eq!(out, "n");
}

/// Verifies `is_callable` correctly resolves a dynamic string (function argument) that names a
/// built-in function, demonstrating runtime catalog lookup.
#[test]
fn test_is_callable_dynamic_builtin_string_returns_true() {
    let out = compile_and_run(
        r#"<?php
            function check(string $name) {
                return is_callable($name) ? "y" : "n";
            }
            echo check("JSON_ENCODE");
        "#,
    );
    assert_eq!(out, "y");
}

/// Verifies `is_callable` correctly resolves a dynamic string (function argument) that names a
/// user-defined function, demonstrating runtime catalog lookup for user functions.
#[test]
fn test_is_callable_dynamic_user_function_string_returns_true() {
    let out = compile_and_run(
        r#"<?php
            function target_fn() { return 1; }
            function check(string $name) {
                return is_callable($name) ? "y" : "n";
            }
            echo check("target_fn");
        "#,
    );
    assert_eq!(out, "y");
}

/// Verifies `is_callable` returns false when a dynamic string names a function that does not exist.
#[test]
fn test_is_callable_dynamic_unknown_string_returns_false() {
    let out = compile_and_run(
        r#"<?php
            function check(string $name) {
                return is_callable($name) ? "y" : "n";
            }
            echo check("missing_callable_name");
        "#,
    );
    assert_eq!(out, "n");
}

/// Verifies `is_callable` returns true for the string `"ClassName::staticMethod"` when the
/// method exists and is public static.
#[test]
fn test_is_callable_dynamic_static_method_string_returns_true() {
    let out = compile_and_run(
        r#"<?php
            class MathBox {
                public static function double($n) {
                    return $n * 2;
                }
            }
            function check(string $name) {
                return is_callable($name) ? "y" : "n";
            }
            echo check("MathBox::double");
        "#,
    );
    assert_eq!(out, "y");
}

/// Verifies is callable object method array returns true.
#[test]
fn test_is_callable_object_method_array_returns_true() {
    let out = compile_and_run(
        r#"<?php
            class Greeter {
                public function hello() {
                    return "hi";
                }
            }
            $obj = new Greeter();
            $cb = [$obj, "hello"];
            echo is_callable($cb) ? "y" : "n";
        "#,
    );
    assert_eq!(out, "y");
}

/// Verifies is callable inherited object method array returns true.
#[test]
fn test_is_callable_inherited_object_method_array_returns_true() {
    let out = compile_and_run(
        r#"<?php
            class BaseGreeter {
                public function hello() {
                    return "hi";
                }
            }
            class ChildGreeter extends BaseGreeter {}
            $obj = new ChildGreeter();
            $cb = [$obj, "hello"];
            echo is_callable($cb) ? "y" : "n";
        "#,
    );
    assert_eq!(out, "y");
}

/// Verifies is callable object method array missing method returns false.
#[test]
fn test_is_callable_object_method_array_missing_method_returns_false() {
    let out = compile_and_run(
        r#"<?php
            class Greeter {
                public function hello() {
                    return "hi";
                }
            }
            $obj = new Greeter();
            $cb = [$obj, "missing"];
            echo is_callable($cb) ? "y" : "n";
        "#,
    );
    assert_eq!(out, "n");
}

/// Verifies is callable class string static method array returns true.
#[test]
fn test_is_callable_class_string_static_method_array_returns_true() {
    let out = compile_and_run(
        r#"<?php
            class MathBox {
                public static function double($n) {
                    return $n * 2;
                }
            }
            $class = "MathBox";
            $cb = [$class, "double"];
            echo is_callable($cb) ? "y" : "n";
        "#,
    );
    assert_eq!(out, "y");
}

/// Verifies is callable class string static method array is case insensitive.
#[test]
fn test_is_callable_class_string_static_method_array_is_case_insensitive() {
    let out = compile_and_run(
        r#"<?php
            class MathBox {
                public static function double($n) {
                    return $n * 2;
                }
            }
            $class = "mathbox";
            $cb = [$class, "DOUBLE"];
            echo is_callable($cb) ? "y" : "n";
        "#,
    );
    assert_eq!(out, "y");
}

/// Verifies is callable class string static method array missing returns false.
#[test]
fn test_is_callable_class_string_static_method_array_missing_returns_false() {
    let out = compile_and_run(
        r#"<?php
            class MathBox {
                public static function double($n) {
                    return $n * 2;
                }
            }
            $class = "MathBox";
            $cb = [$class, "missing"];
            echo is_callable($cb) ? "y" : "n";
        "#,
    );
    assert_eq!(out, "n");
}

/// Verifies is callable class string static method array rejects non public.
#[test]
fn test_is_callable_class_string_static_method_array_rejects_non_public() {
    let out = compile_and_run(
        r#"<?php
            class MathBox {
                protected static function hidden() {
                    return 2;
                }
            }
            $class = "MathBox";
            $cb = [$class, "hidden"];
            echo is_callable($cb) ? "y" : "n";
        "#,
    );
    assert_eq!(out, "n");
}

/// Verifies is callable invokable object returns true.
#[test]
fn test_is_callable_invokable_object_returns_true() {
    let out = compile_and_run(
        r#"<?php
            class Task {
                public function __invoke() {
                    return 1;
                }
            }
            $task = new Task();
            echo is_callable($task) ? "y" : "n";
        "#,
    );
    assert_eq!(out, "y");
}

/// Verifies is callable inherited invokable object returns true.
#[test]
fn test_is_callable_inherited_invokable_object_returns_true() {
    let out = compile_and_run(
        r#"<?php
            class BaseTask {
                public function __invoke() {
                    return 1;
                }
            }
            class ChildTask extends BaseTask {}
            $task = new ChildTask();
            echo is_callable($task) ? "y" : "n";
        "#,
    );
    assert_eq!(out, "y");
}

/// Verifies is callable plain object returns false.
#[test]
fn test_is_callable_plain_object_returns_false() {
    let out = compile_and_run(
        r#"<?php
            class Task {
                public function run() {
                    return 1;
                }
            }
            $task = new Task();
            echo is_callable($task) ? "y" : "n";
        "#,
    );
    assert_eq!(out, "n");
}
