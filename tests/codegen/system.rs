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

/// Verifies `date("y")` returns the zero-padded two-digit year. Built via `mktime()` so the
/// timestamp round-trips through the same local timezone and the result is machine-independent.
#[test]
fn test_date_two_digit_year() {
    let out = compile_and_run("<?php echo date(\"y\", mktime(0, 0, 0, 6, 15, 2024));");
    assert_eq!(out, "24");
}

/// Verifies `date("h")` returns the zero-padded 12-hour clock value, mapping 15:00 → "03"
/// and midnight → "12". Uses `mktime()` for a timezone-independent round-trip.
#[test]
fn test_date_12_hour_padded() {
    let out = compile_and_run(
        "<?php echo date(\"h\", mktime(15, 30, 0, 6, 15, 2024)) . date(\"h\", mktime(0, 0, 0, 6, 15, 2024));",
    );
    assert_eq!(out, "0312");
}

/// Verifies `date("w")` returns the numeric weekday with Sunday=0; 2024-06-15 is a Saturday → "6".
#[test]
fn test_date_numeric_weekday() {
    let out = compile_and_run("<?php echo date(\"w\", mktime(12, 0, 0, 6, 15, 2024));");
    assert_eq!(out, "6");
}

/// Verifies `date("z")` returns the zero-based day of year: Jan 1 → "0" and 2024-03-01 → "60"
/// (31 January + 29 leap February days). Uses `mktime()` for a timezone-independent round-trip.
#[test]
fn test_date_day_of_year() {
    let out = compile_and_run(
        "<?php echo date(\"z\", mktime(0, 0, 0, 1, 1, 2024)) . \"|\" . date(\"z\", mktime(12, 0, 0, 3, 1, 2024));",
    );
    assert_eq!(out, "0|60");
}

/// Verifies `date("S")` appends the correct English ordinal suffix, covering the st/nd/rd cases,
/// the 11–13 "th" exception, and the 21 → "st" wrap. Combined with `j` to mirror typical usage.
#[test]
fn test_date_ordinal_suffix() {
    let out = compile_and_run(
        "<?php \
echo date(\"jS\", mktime(12, 0, 0, 6, 1, 2024)) . \"|\" \
. date(\"jS\", mktime(12, 0, 0, 6, 2, 2024)) . \"|\" \
. date(\"jS\", mktime(12, 0, 0, 6, 3, 2024)) . \"|\" \
. date(\"jS\", mktime(12, 0, 0, 6, 11, 2024)) . \"|\" \
. date(\"jS\", mktime(12, 0, 0, 6, 21, 2024));",
    );
    assert_eq!(out, "1st|2nd|3rd|11th|21st");
}

/// Verifies `date("t")` returns the number of days in the month, including the leap-year
/// February adjustment (29 vs 28) and the 30/31-day months.
#[test]
fn test_date_days_in_month() {
    let out = compile_and_run(
        "<?php \
echo date(\"t\", mktime(12, 0, 0, 2, 15, 2024)) . \"|\" \
. date(\"t\", mktime(12, 0, 0, 2, 15, 2023)) . \"|\" \
. date(\"t\", mktime(12, 0, 0, 4, 15, 2024)) . \"|\" \
. date(\"t\", mktime(12, 0, 0, 1, 15, 2024));",
    );
    assert_eq!(out, "29|28|30|31");
}

/// Verifies `date("L")` returns the leap-year flag, exercising the divisible-by-4 (2024 → 1),
/// common-year (2023 → 0), and divisible-by-400 (2000 → 1) branches of the leap-year rule.
#[test]
fn test_date_leap_year_flag() {
    let out = compile_and_run(
        "<?php \
echo date(\"L\", mktime(12, 0, 0, 6, 15, 2024)) \
. date(\"L\", mktime(12, 0, 0, 6, 15, 2023)) \
. date(\"L\", mktime(12, 0, 0, 6, 15, 2000));",
    );
    assert_eq!(out, "101");
}

/// Verifies `date("W")` returns the zero-padded ISO-8601 week number for a mid-year date
/// (2024-06-15 is in ISO week 24).
#[test]
fn test_date_iso_week() {
    let out = compile_and_run("<?php echo date(\"W\", mktime(12, 0, 0, 6, 15, 2024));");
    assert_eq!(out, "24");
}

/// Verifies `date("W")`/`date("o")` at the year boundaries, where the ISO week-numbering year
/// differs from the calendar year: 2024-12-31 is in week 01 of 2025; 2021-01-01 is in week 53 of
/// 2020; 2023-01-01 is in week 52 of 2022.
#[test]
fn test_date_iso_week_year_boundaries() {
    let out = compile_and_run(
        "<?php \
echo date(\"W\", mktime(12, 0, 0, 12, 31, 2024)) . \"/\" . date(\"o\", mktime(12, 0, 0, 12, 31, 2024)) . \"|\" \
. date(\"W\", mktime(12, 0, 0, 1, 1, 2021)) . \"/\" . date(\"o\", mktime(12, 0, 0, 1, 1, 2021)) . \"|\" \
. date(\"W\", mktime(12, 0, 0, 1, 1, 2023)) . \"/\" . date(\"o\", mktime(12, 0, 0, 1, 1, 2023));",
    );
    assert_eq!(out, "01/2025|53/2020|52/2022");
}

/// Verifies `gmdate()` formats a fixed timestamp in UTC. 1700000000 is 2023-11-14 22:13:20 UTC,
/// so the result is exact and machine-timezone-independent (unlike `date()`).
#[test]
fn test_gmdate_full_format() {
    let out = compile_and_run("<?php echo gmdate(\"Y-m-d H:i:s\", 1700000000);");
    assert_eq!(out, "2023-11-14 22:13:20");
}

/// Verifies `gmdate()` uses UTC regardless of the machine timezone: the Unix epoch (0) formats
/// to 1970-01-01 00:00:00, which would shift a day/hour under `date()` in a non-UTC zone.
#[test]
fn test_gmdate_epoch_is_utc() {
    let out = compile_and_run("<?php echo gmdate(\"Y-m-d H:i:s\", 0);");
    assert_eq!(out, "1970-01-01 00:00:00");
}

/// Regression: `gmdate("T")` must report `"GMT"` (PHP's UTC abbreviation for the GMT path) on every
/// target, while `date("T")` in the UTC default zone reports `"UTC"`. macOS `gmtime()` sets
/// `tm_zone = "UTC"`, so the gmdate path now emits the literal `"GMT"` instead of trusting libc.
#[test]
fn test_gmdate_t_token_is_gmt() {
    let out = compile_and_run(
        "<?php date_default_timezone_set(\"UTC\"); echo gmdate(\"T\", 0), \"|\", date(\"T\", 0);",
    );
    assert_eq!(out, "GMT|UTC");
}

/// Verifies `gmdate()` formats the leap day 2024-02-29 (timestamp 1709251199 = 23:59:59 UTC).
#[test]
fn test_gmdate_leap_day() {
    let out = compile_and_run("<?php echo gmdate(\"Y-m-d\", 1709251199);");
    assert_eq!(out, "2024-02-29");
}

/// Verifies the newly added specifiers (`y`, `n`, `j`, `g`, `i`, `A`) resolve through `gmdate()`
/// against a fixed UTC timestamp (2023-11-14 22:13:20 → 10:13 PM in 12-hour form).
#[test]
fn test_gmdate_new_specifiers() {
    let out = compile_and_run("<?php echo gmdate(\"y n j g i A\", 1700000000);");
    assert_eq!(out, "23 11 14 10 13 PM");
}

/// Verifies the calendar specifiers (`N`, `w`, `z`, `t`, `L`) through `gmdate()`: 2023-11-14 is a
/// Tuesday (N=2, w=2), day-of-year 317, November has 30 days, and 2023 is not a leap year.
#[test]
fn test_gmdate_calendar_specifiers() {
    let out = compile_and_run("<?php echo gmdate(\"N w z t L\", 1700000000);");
    assert_eq!(out, "2 2 317 30 0");
}

/// Verifies `gmdate()` is recognized case-insensitively like every PHP builtin (`GmDate`).
#[test]
fn test_gmdate_case_insensitive() {
    let out = compile_and_run("<?php echo GmDate(\"Y-m-d\", 1700000000);");
    assert_eq!(out, "2023-11-14");
}

/// Verifies a backslash in the format string escapes the next character so it is emitted
/// literally — here the ISO 8601 literal `T` between the date and time parts.
#[test]
fn test_date_format_escape_iso8601() {
    let out = compile_and_run("<?php echo gmdate('Y-m-d\\TH:i:s', 1700000000);");
    assert_eq!(out, "2023-11-14T22:13:20");
}

/// Verifies escaped specifier letters are emitted literally inside words (`\o\f` → "of"),
/// while unescaped specifiers still expand (`jS` → "14th", `F` → "November").
#[test]
fn test_date_format_escape_words() {
    let out = compile_and_run("<?php echo gmdate('jS \\o\\f F', 1700000000);");
    assert_eq!(out, "14th of November");
}

/// Verifies an escaped specifier is a literal while the same unescaped letter still expands:
/// `\Y` → "Y" but `Y` → "2023".
#[test]
fn test_date_format_escaped_vs_real_specifier() {
    let out = compile_and_run("<?php echo gmdate('\\Y=Y', 1700000000);");
    assert_eq!(out, "Y=2023");
}

/// Verifies escapes work through `date()` too: an all-literal escaped format is timezone
/// independent, so `\Y\e\s` renders "Yes" regardless of the machine timezone.
#[test]
fn test_date_format_escape_all_literal() {
    let out = compile_and_run("<?php echo date('\\Y\\e\\s', 1700000000);");
    assert_eq!(out, "Yes");
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

/// Verifies `mktime`/`gmmktime` handle years before 1900 (which libc rejects) via a 400-year
/// Gregorian-cycle shift, matching PHP's negative timestamps; an in-range year (1900) is unchanged.
#[test]
fn test_mktime_pre_1900() {
    let out = compile_and_run(
        "<?php
date_default_timezone_set(\"UTC\");
echo mktime(0, 0, 0, 3, 15, 1850), \"|\", date(\"Y-m-d\", mktime(0, 0, 0, 3, 15, 1850)), \"|\",
     gmmktime(0, 0, 0, 7, 4, 1776), \"|\", gmdate(\"Y-m-d\", gmmktime(0, 0, 0, 7, 4, 1776)), \"|\",
     mktime(0, 0, 0, 1, 1, 1900);
",
    );
    assert_eq!(out, "-3780518400|1850-03-15|-6106060800|1776-07-04|-2208988800");
}

/// Verifies `mktime()`/`gmmktime()` apply PHP's two-digit-year shorthand: years
/// 0-69 map to 2000-2069, years 70-100 map to 1970-2000, and years >= 101 are
/// taken literally (101 stays year 101, not 2001).
#[test]
fn test_mktime_2digit_year() {
    let out = compile_and_run(
        "<?php
date_default_timezone_set(\"UTC\");
echo mktime(0, 0, 0, 1, 1, 99), \"|\", date(\"Y\", mktime(0, 0, 0, 1, 1, 99)), \"|\",
     mktime(0, 0, 0, 1, 1, 50), \"|\", date(\"Y\", mktime(0, 0, 0, 1, 1, 50)), \"|\",
     mktime(0, 0, 0, 1, 1, 70), \"|\", mktime(0, 0, 0, 1, 1, 69), \"|\",
     gmmktime(0, 0, 0, 6, 15, 99);
",
    );
    assert_eq!(out, "915148800|1999|2524608000|2050|0|3124224000|929404800");
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

/// Regression: `mktime()` must unbox `Mixed`/`Union` arguments before building the timestamp.
/// Heterogeneous-array values (`$a["d"]`, `$b["mo"]`) are boxed Mixed cells; before the fix the
/// emitter pushed the boxed pointer as a raw integer, producing wildly wrong timestamps. The
/// subtraction is timezone-independent (both calls use the same zone), so the result must be 0.
#[test]
fn test_mktime_unboxes_mixed_args() {
    let out = compile_and_run(
        r#"<?php
$a = ["d" => 15, "tag" => "x"];
$b = ["mo" => 6, "tag" => "y"];
echo mktime(0, 0, 0, $b["mo"], $a["d"], 2020) - mktime(0, 0, 0, 6, 15, 2020);
"#,
    );
    assert_eq!(out, "0");
}

/// Regression: `date()` must unbox a `Mixed`/`Union` timestamp argument before formatting.
/// `$a["ts"]` is a boxed Mixed cell; before the fix the emitter passed the boxed pointer as the
/// raw timestamp. Comparing `date(..., Mixed)` to `date(..., literal)` is timezone-independent.
#[test]
fn test_date_unboxes_mixed_timestamp() {
    let out = compile_and_run(
        r#"<?php
$a = ["ts" => 0, "tag" => "x"];
echo date("Y", $a["ts"]) === date("Y", 0) ? "ok" : "bad";
"#,
    );
    assert_eq!(out, "ok");
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

/// Verifies `strtotime` returns `false` (PHP's failure value) for malformed ISO-like strings
/// that have extra junk after the datetime; a strict `=== false` check distinguishes failure
/// from any valid timestamp.
#[test]
fn test_strtotime_rejects_malformed_iso_datetime() {
    let out = compile_and_run(
        r#"<?php
echo (strtotime("2024-06-15 12:30:45 extra") === false ? "F" : "x") . ",";
echo (strtotime("2024-06-15abc") === false ? "F" : "x") . ",";
echo (strtotime("2024-06-15 12:30x") === false ? "F" : "x") . ",";
echo (strtotime("2024-06-15 12") === false ? "F" : "x") . ",";
echo (strtotime("2024/06/15") === false ? "F" : "x") . ",";
echo (strtotime("2024-0x-15") === false ? "F" : "x");
"#,
    );
    assert_eq!(out, "F,F,F,F,F,F");
}

/// Verifies the strtotime() failure value is `false`, not `-1`: a failed parse echoes as the
/// empty string and satisfies `=== false`, while `-1` remains a real timestamp — one second
/// before the epoch — reachable via an explicit UTC date string.
#[test]
fn test_strtotime_false_on_failure_minus_one_is_valid() {
    let out = compile_and_run(
        r#"<?php
echo strtotime("complete garbage"), "|";
echo (strtotime("complete garbage") === false ? "1" : "0"), "|";
echo strtotime("1969-12-31 23:59:59 UTC"), "|";
echo (strtotime("1969-12-31 23:59:59 UTC") === -1 ? "1" : "0");
"#,
    );
    assert_eq!(out, "|1|-1|1");
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

/// Verifies the 2-argument `strtotime(modifier, base)`: `"now"` relative to a base timestamp
/// returns the base unchanged (timezone-independent).
#[test]
fn test_strtotime_base_now_returns_base() {
    let out = compile_and_run("<?php echo strtotime(\"now\", 1700000000);");
    assert_eq!(out, "1700000000");
}

/// Verifies `strtotime("+2 hours", base)` offsets from the supplied base timestamp. 2023-11-14
/// is clear of any DST transition, so the result is exactly base + 7200 in every timezone.
#[test]
fn test_strtotime_base_hour_offset() {
    let out = compile_and_run("<?php echo strtotime(\"+2 hours\", 1700000000);");
    assert_eq!(out, "1700007200");
}

/// Verifies `strtotime("+1 day", base)` offsets a whole day from the base (DST-free date, so
/// exactly base + 86400 in every timezone).
#[test]
fn test_strtotime_base_day_offset() {
    let out = compile_and_run("<?php echo strtotime(\"+1 day\", 1700000000);");
    assert_eq!(out, "1700086400");
}

/// Verifies the `@<timestamp>` epoch form returns the literal UNIX timestamp (UTC, no parsing
/// against the current time).
#[test]
fn test_strtotime_epoch() {
    let out = compile_and_run("<?php echo strtotime(\"@1700000000\");");
    assert_eq!(out, "1700000000");
}

/// Verifies the `@<timestamp>` epoch form accepts zero and negative (pre-epoch) values.
#[test]
fn test_strtotime_epoch_zero_and_negative() {
    let out = compile_and_run("<?php echo strtotime(\"@0\"), \"|\", strtotime(\"@-5\");");
    assert_eq!(out, "0|-5");
}

/// Verifies the `@<timestamp>` epoch form truncates a fractional part (matching PHP).
#[test]
fn test_strtotime_epoch_truncates_fraction() {
    let out = compile_and_run("<?php echo strtotime(\"@1700000000.999\");");
    assert_eq!(out, "1700000000");
}

/// Verifies a malformed epoch (`@` with no digits) reports strtotime's `false` failure value.
#[test]
fn test_strtotime_epoch_invalid() {
    let out = compile_and_run("<?php echo strtotime(\"@abc\") === false ? \"F\" : \"x\";");
    assert_eq!(out, "F");
}

/// Verifies the American `MM/DD/YYYY` slash-date form. The result is built with `mktime` and
/// reformatted with `date()`, so the date round-trips through the local zone (machine-independent).
#[test]
fn test_strtotime_slash_date() {
    let out = compile_and_run("<?php echo date(\"Y-m-d\", strtotime(\"12/25/2024\"));");
    assert_eq!(out, "2024-12-25");
}

/// Verifies single-digit month/day slash dates (`M/D/YYYY`).
#[test]
fn test_strtotime_slash_single_digit() {
    let out = compile_and_run("<?php echo date(\"Y-m-d\", strtotime(\"1/5/2024\"));");
    assert_eq!(out, "2024-01-05");
}

/// Verifies PHP's 2-digit-year windowing for slash dates: `24` → 2024 but `70` → 1970.
#[test]
fn test_strtotime_slash_two_digit_year() {
    let out = compile_and_run(
        "<?php echo date(\"Y\", strtotime(\"12/25/24\")) . \"|\" . date(\"Y\", strtotime(\"12/25/70\"));",
    );
    assert_eq!(out, "2024|1970");
}

/// Verifies a slash date with an `HH:MM` time suffix sets the clock.
#[test]
fn test_strtotime_slash_with_time() {
    let out = compile_and_run(
        "<?php echo date(\"Y-m-d H:i:s\", strtotime(\"6/15/2024 8:05\"));",
    );
    assert_eq!(out, "2024-06-15 08:05:00");
}

/// Verifies a slash date with an out-of-range month is rejected with the `false` failure value.
#[test]
fn test_strtotime_slash_rejects_bad_month() {
    let out = compile_and_run("<?php echo strtotime(\"13/01/2024\") === false ? \"F\" : \"x\";");
    assert_eq!(out, "F");
}

/// Verifies the textual day-first form `D Month Y` (full month name).
#[test]
fn test_strtotime_textual_day_first() {
    let out = compile_and_run("<?php echo date(\"Y-m-d\", strtotime(\"25 December 2024\"));");
    assert_eq!(out, "2024-12-25");
}

/// Verifies the textual month-first form `Month D, Y` (with and without the comma).
#[test]
fn test_strtotime_textual_month_first() {
    let out = compile_and_run(
        "<?php echo date(\"Y-m-d\", strtotime(\"December 25, 2024\")) . \"|\" . date(\"Y-m-d\", strtotime(\"December 25 2024\"));",
    );
    assert_eq!(out, "2024-12-25|2024-12-25");
}

/// Verifies abbreviated and mixed-case month names parse (matching is case-insensitive).
#[test]
fn test_strtotime_textual_abbrev_and_case() {
    let out = compile_and_run(
        "<?php echo date(\"Y-m-d\", strtotime(\"25 Dec 2024\")) . \"|\" . date(\"Y-m-d\", strtotime(\"25 DECEMBER 2024\"));",
    );
    assert_eq!(out, "2024-12-25|2024-12-25");
}

/// Verifies a textual date with an `HH:MM` time suffix sets the clock.
#[test]
fn test_strtotime_textual_with_time() {
    let out = compile_and_run(
        "<?php echo date(\"Y-m-d H:i:s\", strtotime(\"25 December 2024 14:30\"));",
    );
    assert_eq!(out, "2024-12-25 14:30:00");
}

/// Verifies a textual date with an out-of-range day is normalized by `mktime` (`31 feb` → Mar 2),
/// matching PHP (no day validation for textual dates).
#[test]
fn test_strtotime_textual_normalizes_overflow() {
    let out = compile_and_run("<?php echo date(\"Y-m-d\", strtotime(\"31 feb 2024\"));");
    assert_eq!(out, "2024-03-02");
}

/// Verifies the day-first textual path falls back to the relative-offset parser when the word
/// after the number is a unit rather than a month: `strtotime("2 weeks", base)` = base + 14 days.
#[test]
fn test_strtotime_textual_offset_fallback() {
    let out = compile_and_run("<?php echo strtotime(\"2 weeks\", 1700000000);");
    assert_eq!(out, "1701209600");
}

/// Regression: `date()` accepts a boxed `Mixed` format argument (a `foreach` loop variable over a
/// string array) by coercing it through `__rt_mixed_cast_string` instead of rejecting it with
/// "date format for PHP type Mixed". Each format must render against the same fixed timestamp.
#[test]
fn test_date_mixed_format_from_foreach() {
    let out = compile_and_run(
        "<?php $fmts = [\"Y-m-d\", \"H:i:s\", \"D\"]; foreach ($fmts as $f) { echo date($f, 1700000000), \"|\"; }",
    );
    assert_eq!(out, "2023-11-14|22:13:20|Tue|");
}

/// Regression: `gmdate()` likewise accepts a boxed `Mixed` format argument, rendering the instant
/// in UTC for each format pulled from a `foreach` over a string array.
#[test]
fn test_gmdate_mixed_format_from_foreach() {
    let out = compile_and_run(
        "<?php $fmts = [\"Y-m-d\", \"H:i:s\"]; foreach ($fmts as $f) { echo gmdate($f, 1700000000), \"|\"; }",
    );
    assert_eq!(out, "2023-11-14|22:13:20|");
}

/// Regression: `strtotime()` accepts a boxed `Mixed` datetime argument (a `foreach` loop variable
/// over a string array) by coercing it to a string, instead of failing with
/// "strtotime for PHP type Mixed".
#[test]
fn test_strtotime_mixed_datetime_from_foreach() {
    let out = compile_and_run(
        "<?php $strs = [\"2020-01-01\", \"@1000000000\"]; foreach ($strs as $s) { echo strtotime($s), \"|\"; }",
    );
    assert_eq!(out, "1577836800|1000000000|");
}

/// Regression: `strtotime()` rejects ISO date/time fields whose value is outside PHP/timelib's
/// per-field regex bounds (month ≤ 12, day ≤ 31, hour ≤ 24, minute ≤ 59, second ≤ 60) by returning
/// `false`, instead of silently normalizing them through `mktime`. In-range calendar overflow such
/// as `02-30` must still normalize, and month/day `0` must still be accepted as PHP does.
#[test]
fn test_strtotime_rejects_out_of_range_iso_fields() {
    let src = "<?php $c = [\"2026-13-45\", \"2026-12-32\", \"2026-01-01 25:00:00\", \
        \"2026-01-01 12:60:00\", \"2026-01-01 12:00:61\"]; \
        foreach ($c as $s) { $r = strtotime($s . \" UTC\"); echo ($r === false) ? \"F\" : \"T\"; }";
    assert_eq!(compile_and_run(src), "FFFFF");
}

/// Regression companion: ISO date/time values that PHP accepts must keep working — month/day `0`
/// (normalized to the adjacent day), in-range calendar overflow (`02-30` → Mar 2), the `24:00`
/// hour, and the leap `:60` second all parse to a concrete timestamp rather than `false`.
#[test]
fn test_strtotime_accepts_in_range_and_normalized_iso_fields() {
    let src = "<?php $c = [\"2026-00-10 00:00:00 UTC\", \"2026-02-30 00:00:00 UTC\", \
        \"2026-01-01 24:00:00 UTC\", \"2026-01-01 12:00:60 UTC\"]; \
        foreach ($c as $s) { $r = strtotime($s); echo ($r === false) ? \"F\" : \"T\"; }";
    assert_eq!(compile_and_run(src), "TTTT");
}

/// Verifies `first/last day of this month` resolve to day 1 and the month's last day. The base is
/// built with `mktime` so the result round-trips through the local zone (machine-independent).
#[test]
fn test_strtotime_first_last_day_of_this_month() {
    let out = compile_and_run(
        "<?php $b = mktime(0, 0, 0, 6, 15, 2024); echo date(\"Y-m-d\", strtotime(\"first day of this month\", $b)) . \"|\" . date(\"Y-m-d\", strtotime(\"last day of this month\", $b));",
    );
    assert_eq!(out, "2024-06-01|2024-06-30");
}

/// Verifies the month modifiers `next` and `last`/`previous` shift the target month.
#[test]
fn test_strtotime_first_last_day_of_other_month() {
    let out = compile_and_run(
        "<?php $b = mktime(0, 0, 0, 6, 15, 2024); echo date(\"Y-m-d\", strtotime(\"first day of next month\", $b)) . \"|\" . date(\"Y-m-d\", strtotime(\"last day of next month\", $b)) . \"|\" . date(\"Y-m-d\", strtotime(\"last day of previous month\", $b));",
    );
    assert_eq!(out, "2024-07-01|2024-07-31|2024-05-31");
}

/// Verifies `first/last day of` preserves the base time of day (it only changes the calendar day).
#[test]
fn test_strtotime_first_day_preserves_time() {
    let out = compile_and_run(
        "<?php echo date(\"Y-m-d H:i:s\", strtotime(\"first day of this month\", mktime(14, 30, 45, 6, 15, 2024)));",
    );
    assert_eq!(out, "2024-06-01 14:30:45");
}

/// Verifies that routing `last` through the first/last-day strategy still falls back to the
/// weekday parser, so `last monday` keeps working (2024-06-15 is a Saturday → the 10th).
#[test]
fn test_strtotime_last_weekday_still_works() {
    let out = compile_and_run(
        "<?php echo date(\"Y-m-d\", strtotime(\"last monday\", mktime(0, 0, 0, 6, 15, 2024)));",
    );
    assert_eq!(out, "2024-06-10");
}

/// Verifies `nth weekday of <modifier> month`: the first, second, and last Monday of June 2024
/// (the 3rd, 10th, and 24th). Anchored with `mktime` so it round-trips through the local zone.
#[test]
fn test_strtotime_nth_weekday_of_month() {
    let out = compile_and_run(
        "<?php $b = mktime(12, 0, 0, 6, 15, 2024); echo date(\"Y-m-d\", strtotime(\"first monday of this month\", $b)) . \"|\" . date(\"Y-m-d\", strtotime(\"second monday of this month\", $b)) . \"|\" . date(\"Y-m-d\", strtotime(\"last monday of this month\", $b));",
    );
    assert_eq!(out, "2024-06-03|2024-06-10|2024-06-24");
}

/// Verifies the month modifier and other weekdays/ordinals: `last friday of this month` (28th),
/// `third tuesday of next month` (2024-07-16), and `last sunday of last month` (2024-05-26).
#[test]
fn test_strtotime_nth_weekday_modifiers() {
    let out = compile_and_run(
        "<?php $b = mktime(12, 0, 0, 6, 15, 2024); echo date(\"Y-m-d\", strtotime(\"last friday of this month\", $b)) . \"|\" . date(\"Y-m-d\", strtotime(\"third tuesday of next month\", $b)) . \"|\" . date(\"Y-m-d\", strtotime(\"last sunday of last month\", $b));",
    );
    assert_eq!(out, "2024-06-28|2024-07-16|2024-05-26");
}

/// Verifies a `fifth` occurrence that overflows the month rolls into the next month, matching
/// PHP: June 2024 has only four Mondays, so `fifth monday of this month` is 2024-07-01.
#[test]
fn test_strtotime_nth_weekday_overflow() {
    let out = compile_and_run(
        "<?php echo date(\"Y-m-d\", strtotime(\"fifth monday of this month\", mktime(12, 0, 0, 6, 15, 2024)));",
    );
    assert_eq!(out, "2024-07-01");
}

/// Verifies nth-weekday resets the time to midnight (unlike `first day of`, which preserves it).
#[test]
fn test_strtotime_nth_weekday_resets_time() {
    let out = compile_and_run(
        "<?php echo date(\"H:i:s\", strtotime(\"first monday of this month\", mktime(14, 30, 45, 6, 15, 2024)));",
    );
    assert_eq!(out, "00:00:00");
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

/// Verifies `strtotime` returns `false` for a non-parseable string like "garbage": the value
/// echoes as the empty string and satisfies a strict `=== false` comparison through a local.
#[test]
fn test_strtotime_invalid() {
    let out = compile_and_run(
        r#"<?php
$ts = strtotime("garbage");
echo $ts, "|", ($ts === false ? "F" : "x");
"#,
    );
    assert_eq!(out, "|F");
}

/// Verifies `strtotime` rejects keywords and weekday names with trailing junk characters
/// (e.g. "today123", "today!", "Monday2", "next Monday2") by returning `false`.
#[test]
fn test_strtotime_rejects_keyword_and_weekday_suffix_junk() {
    let out = compile_and_run(
        r#"<?php
echo (strtotime("today123") === false ? "F" : "x") . ",";
echo (strtotime("today!") === false ? "F" : "x") . ",";
echo (strtotime("Monday2") === false ? "F" : "x") . ",";
echo (strtotime("next Monday2") === false ? "F" : "x");
"#,
    );
    assert_eq!(out, "F,F,F,F");
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

/// Verifies `strtotime` returns `false` for invalid time-only shapes (junk suffix, out-of-range values, malformed separator).
#[test]
fn test_strtotime_rejects_invalid_time_only_shapes() {
    let out = compile_and_run(
        r#"<?php
echo (strtotime("14:30abc") === false ? "F" : "x") . ",";
echo (strtotime("14:30:99") === false ? "F" : "x") . ",";
echo (strtotime("99:99") === false ? "F" : "x") . ",";
echo (strtotime("14:30:") === false ? "F" : "x");
"#,
    );
    assert_eq!(out, "F,F,F,F");
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

/// Verifies the relative `this/next/last <unit>` forms for non-week units: `this`=+0, `next`=+1,
/// `last`=-1 of the unit applied to the base timestamp (calendar arithmetic for month/year),
/// previously unsupported (returned -1). Also confirms the fall-back still parses
/// `next <weekday>` correctly (the unit intercept must not break the weekday strategy).
#[test]
fn test_strtotime_this_next_last_unit() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("UTC");
$b = 1710511800;
echo strtotime("next month", $b), "|", strtotime("last year", $b), "|",
     strtotime("this hour", $b), "|", strtotime("next day", $b), "|",
     strtotime("last minute", $b), "|", strtotime("next monday", $b);
"#,
    );
    assert_eq!(
        out,
        "1713190200|1678889400|1710511800|1710598200|1710511740|1710720000"
    );
}

/// Verifies the Monday-anchored `this/next/last week` relative forms: the result is the Monday
/// of this/next/last week (this = `-((ISO weekday)-1)` days; next/last add ±1 week) keeping the
/// time-of-day, per PHP. Base is a Friday (2024-03-15 14:10 UTC).
#[test]
fn test_strtotime_relative_week() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("UTC");
$b = 1710511800;
echo strtotime("this week", $b), "|", strtotime("next week", $b), "|",
     strtotime("last week", $b);
"#,
    );
    assert_eq!(out, "1710166200|1710771000|1709561400");
}

/// Verifies a trailing `UTC`/`GMT` word on ISO 8601 input is treated as explicit UTC (offset 0),
/// overriding a non-UTC default zone, while a bare parse still uses the default. Base default is
/// Europe/Paris (+02:00 in summer), so the bare value is offset from the UTC ones.
#[test]
fn test_strtotime_zone_word_utc_gmt() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("Europe/Paris");
echo strtotime("2024-06-15 12:00:00 UTC"), "|", strtotime("2024-06-15 12:00:00 GMT"),
     "|", strtotime("2024-06-15 12:00:00");
"#,
    );
    assert_eq!(out, "1718452800|1718452800|1718445600");
}

/// Verifies `strtotime()` parses a trailing IANA timezone name (e.g. `America/New_York`,
/// `Europe/Paris`) and interprets the wall-clock in that zone, restoring the previous default
/// afterwards. June 15 is EDT (UTC-4) in New York and CEST (UTC+2) in Paris, so the two named
/// results bracket the bare parse, which keeps the default UTC zone — the bare case also proves
/// the zone scan ignores the digit-only time token (no letter) instead of treating it as a zone.
/// Requires IANA tzdata (present on macOS; mounted into the Linux test images).
#[test]
fn test_strtotime_iana_zone_name() {
    let out = compile_and_run(
        r#"<?php
echo strtotime("2024-06-15 12:00:00 America/New_York"), "|",
     strtotime("2024-06-15 12:00:00 Europe/Paris"), "|",
     strtotime("2024-06-15 12:00:00"), "|",
     date_default_timezone_get();
"#,
    );
    assert_eq!(out, "1718467200|1718445600|1718452800|UTC");
}

/// Verifies `strtotime()` honors an explicit trailing timezone offset in ISO 8601 input:
/// `+HH:MM`, `-HH:MM`, space-separated `+HHMM`, and `Z` (UTC). The wall-clock is interpreted
/// in the stated offset (so the result is offset from a bare parse), overriding the default
/// zone — confirmed by repeating `Z` and a bare parse under a non-UTC default. Previously these
/// returned -1.
#[test]
fn test_strtotime_iso_explicit_offset() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("UTC");
$a = strtotime("2024-06-15T12:00:00+02:00") . "|" . strtotime("2024-06-15T12:00:00-05:00")
   . "|" . strtotime("2024-06-15 12:00:00 +0200") . "|" . strtotime("2024-06-15T12:00:00Z")
   . "|" . strtotime("2024-06-15 12:00:00");
date_default_timezone_set("Europe/Paris");
echo $a . "|" . strtotime("2024-06-15T12:00:00Z") . "|" . strtotime("2024-06-15 12:00:00");
"#,
    );
    assert_eq!(
        out,
        "1718445600|1718470800|1718445600|1718452800|1718452800|1718452800|1718445600"
    );
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

/// Regression: `json_encode` of floats needing exponential notation keeps PHP's
/// json layout — a lowercase `e` exponent (`1.0e+20`). The shared
/// `__rt_json_ftoa` now takes the exponent marker as a parameter so `serialize`
/// can emit `'E'`; this guards json's lowercase `'e'` against regressing.
/// Covers positive/negative mantissa, negative exponent, and a 3-digit exponent.
#[test]
fn test_json_encode_float_exponential_lowercase_e() {
    let out = compile_and_run(
        r#"<?php
echo json_encode(1e20), "\n";
echo json_encode(1.5e-10), "\n";
echo json_encode(-2.5e-8), "\n";
echo json_encode(1e100), "\n";
"#,
    );
    assert_eq!(out, "1.0e+20\n1.5e-10\n-2.5e-8\n1.0e+100\n");
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

/// Verifies date_default_timezone_get() returns "UTC" when no default timezone has been set.
#[test]
fn test_date_default_timezone_get_defaults_to_utc() {
    let out = compile_and_run(r#"<?php echo date_default_timezone_get();"#);
    assert_eq!(out, "UTC");
}

/// Verifies date_default_timezone_set() stores the identifier and date_default_timezone_get()
/// returns it verbatim.
#[test]
fn test_date_default_timezone_set_get_roundtrip() {
    let out = compile_and_run(
        r#"<?php date_default_timezone_set("Europe/Paris"); echo date_default_timezone_get();"#,
    );
    assert_eq!(out, "Europe/Paris");
}

/// Verifies date_default_timezone_set() makes date() format in the chosen IANA zone with correct
/// DST offsets (via libc + the system tz database). 2024-07-01 12:00 UTC is summer, so Paris is
/// CEST (+2 → 14:00) and New York is EDT (-4 → 08:00); UTC stays 12:00.
#[test]
fn test_date_default_timezone_set_shifts_date_with_dst() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("Europe/Paris");
echo date("H:i", 1719835200), ",";
date_default_timezone_set("America/New_York");
echo date("H:i", 1719835200), ",";
date_default_timezone_set("UTC");
echo date("H:i", 1719835200);
"#,
    );
    assert_eq!(out, "14:00,08:00,12:00");
}

/// Verifies date_default_timezone_set() returns the PHP boolean true.
#[test]
fn test_date_default_timezone_set_returns_true() {
    let out =
        compile_and_run(r#"<?php echo date_default_timezone_set("Europe/Paris") ? "y" : "n";"#);
    assert_eq!(out, "y");
}

/// Verifies the date() 'P' specifier (UTC offset as +hh:mm) reflects the configured zone and its
/// daylight-saving state: Europe/Paris is CEST (+02:00) on 2024-07-01 but CET (+01:00) on 2024-01-01.
#[test]
fn test_date_offset_specifier_p_paris_summer_winter() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("Europe/Paris");
echo date("P", 1719835200), ",", date("P", 1704110400);
"#,
    );
    assert_eq!(out, "+02:00,+01:00");
}

/// Verifies the date() 'p' specifier: like 'P' for a non-zero offset (+02:00 in Paris summer)
/// but the literal 'Z' when the offset is zero (gmdate, or a UTC default zone).
#[test]
fn test_date_offset_specifier_lower_p_z_for_utc() {
    let out = compile_and_run(
        r#"<?php
echo gmdate("p", 0), ",", date("p", 0), ",";
date_default_timezone_set("Europe/Paris");
echo date("p", 1719835200), ",", date("p", 1704110400);
"#,
    );
    assert_eq!(out, "Z,Z,+02:00,+01:00");
}

/// Verifies the date() 'B' specifier (Swatch Internet Time): beats of the UTC+1 day,
/// zero-padded to three digits, independent of the configured timezone, with floor-mod
/// semantics for pre-epoch timestamps. Values cross-checked against PHP.
#[test]
fn test_date_swatch_beats_specifier() {
    let out = compile_and_run(
        r#"<?php
echo date("B", 0), ",", date("B", -3600), ",", date("B", -7200), ",", date("B", 1719837000), ",";
date_default_timezone_set("Europe/Paris");
echo date("B", 1719837000), ",", gmdate("B", 1719837000);
"#,
    );
    assert_eq!(out, "041,000,958,562,562,562");
}

/// Verifies the date() 'O' (+hhmm, no colon) and 'Z' (offset in seconds) specifiers for a positive
/// (east-of-UTC) zone: Europe/Paris in summer is +0200 / 7200 seconds.
#[test]
fn test_date_offset_specifiers_o_and_z_paris() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("Europe/Paris");
echo date("O", 1719835200), ",", date("Z", 1719835200);
"#,
    );
    assert_eq!(out, "+0200,7200");
}

/// Verifies the offset specifiers render the leading minus sign for a negative (west-of-UTC) zone:
/// America/New_York in summer is EDT, i.e. -04:00 / -0400 / -14400 seconds.
#[test]
fn test_date_offset_specifiers_negative_new_york() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("America/New_York");
echo date("P", 1719835200), ",", date("O", 1719835200), ",", date("Z", 1719835200);
"#,
    );
    assert_eq!(out, "-04:00,-0400,-14400");
}

/// Verifies gmdate()'s offset specifiers are always the UTC zero offset regardless of the configured
/// default zone: '+00:00' / '+0000' / '0'.
#[test]
fn test_gmdate_offset_specifiers_are_utc() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("Europe/Paris");
echo gmdate("P", 1719835200), ",", gmdate("O", 1719835200), ",", gmdate("Z", 1719835200);
"#,
    );
    assert_eq!(out, "+00:00,+0000,0");
}

/// Verifies the offset specifier composes into a full ISO-8601 timestamp (the common date() use case
/// for 'P'). The escaped \T is a literal separator; Paris summer yields the +02:00 offset.
#[test]
fn test_date_offset_specifier_iso8601() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("Europe/Paris");
echo date('Y-m-d\TH:i:sP', 1719835200);
"#,
    );
    assert_eq!(out, "2024-07-01T14:00:00+02:00");
}

/// Verifies elephc defaults the timezone to UTC (matching PHP) when date_default_timezone_set was
/// never called: date()/strtotime()/mktime() of a known UTC instant use the UTC wall clock rather
/// than the host machine's local zone, so output is deterministic regardless of the build host.
#[test]
fn test_timezone_defaults_to_utc_without_set() {
    // 1719835200 = 2024-07-01 12:00:00 UTC. PHP prints 12:00 here without any tz configuration.
    let out = compile_and_run(r#"<?php echo date("Y-m-d H:i", 1719835200);"#);
    assert_eq!(out, "2024-07-01 12:00");
    // mktime() builds the timestamp in the same default (UTC) zone, so it round-trips exactly.
    let rt = compile_and_run(r#"<?php echo mktime(12, 0, 0, 7, 1, 2024);"#);
    assert_eq!(rt, "1719835200");
}

/// Verifies the date() 'e' (timezone identifier) and 'T' (abbreviation) specifiers reflect the
/// configured zone and its daylight-saving state: Europe/Paris is "Europe/Paris" with "CEST" in
/// summer and "CET" in winter.
#[test]
fn test_date_timezone_name_specifiers_paris() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("Europe/Paris");
echo date("e", 1719835200), "|", date("T", 1719835200), "|", date("T", 1704110400);
"#,
    );
    assert_eq!(out, "Europe/Paris|CEST|CET");
}

/// Verifies the 'e' identifier follows the configured zone for date() but is always "UTC" for
/// gmdate() (which formats in UTC regardless of the default zone).
#[test]
fn test_date_e_specifier_gmdate_is_utc() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("America/New_York");
echo date("e", 1719835200), "|", gmdate("e", 1719835200);
"#,
    );
    assert_eq!(out, "America/New_York|UTC");
}

/// Verifies the `I` (daylight-saving flag), `u` (microseconds), and `v` (milliseconds)
/// `date()` specifiers: `I` is 1 only when the zone is in DST at that instant, and `u`/`v`
/// are always all-zero for whole-second Unix timestamps. Previously these emitted the literal
/// specifier letter.
#[test]
fn test_date_specifiers_i_u_v() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("Europe/Paris");
echo date("I", 1705320000), "|", date("I", 1721044800), "|",
     date("u", 1721044800), "|", date("v", 1721044800);
"#,
    );
    assert_eq!(out, "0|1|000000|000");
}

/// Verifies the composite `c` (ISO 8601) and `r` (RFC 2822) `date()` specifiers, which re-run the
/// formatter over a sub-format and so must compose the date, time, and timezone-offset tokens —
/// including the timezone offset (`+02:00` / `+0200`) and correct restoration of the surrounding
/// format (the `[c]` case puts literal brackets around the expansion). Previously both emitted the
/// literal specifier letter.
#[test]
fn test_date_specifiers_c_and_r() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("UTC");
$a = date("c", 951782400) . "|" . date("r", 951782400);
date_default_timezone_set("Europe/Paris");
$b = date("c", 1719835200) . "|" . date("r", 1719835200) . "|" . date("[c]", 1719835200);
echo $a . "|" . $b;
"#,
    );
    assert_eq!(
        out,
        "2000-02-29T00:00:00+00:00|Tue, 29 Feb 2000 00:00:00 +0000|2024-07-01T14:00:00+02:00|Mon, 01 Jul 2024 14:00:00 +0200|[2024-07-01T14:00:00+02:00]"
    );
}

/// Verifies the no-leading-zero specifiers `n` (month) and `G` (24-hour) render without padding,
/// next to their zero-padded counterparts `m`/`H`. Cross-checked against PHP 8.5: `7|07|9|09`.
#[test]
fn test_date_specifiers_n_and_g_no_pad() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("UTC");
$ts = gmmktime(9, 5, 0, 7, 1, 2024);
echo date("n", $ts), "|", date("m", $ts), "|", date("G", $ts), "|", date("H", $ts);
"#,
    );
    assert_eq!(out, "7|07|9|09");
}

/// Verifies gmmktime() builds a Unix timestamp interpreting the components as UTC (libc timegm),
/// independent of the configured default zone — unlike mktime() which uses the local zone.
#[test]
fn test_gmmktime_is_utc() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("Europe/Paris");
echo gmmktime(12, 0, 0, 7, 1, 2024), "|", mktime(12, 0, 0, 7, 1, 2024);
"#,
    );
    assert_eq!(out, "1719835200|1719828000");
}

/// Regression: `gmmktime()` shares `mktime()`'s six-argument marshaller, so it must likewise unbox
/// `Mixed`/`Union` arguments. `$b["mo"]` and `$a["d"]` are boxed heterogeneous-array cells; before
/// the stack-staging marshaller the boxed pointer of one argument was clobbered by the unbox call
/// of a later one. Both calls use UTC, so the difference must be exactly 0.
#[test]
fn test_gmmktime_unboxes_mixed_args() {
    let out = compile_and_run(
        r#"<?php
$a = ["d" => 15, "tag" => "x"];
$b = ["mo" => 6, "tag" => "y"];
echo gmmktime(0, 0, 0, $b["mo"], $a["d"], 2020) - gmmktime(0, 0, 0, 6, 15, 2020);
"#,
    );
    assert_eq!(out, "0");
}

/// Verifies checkdate() validates Gregorian dates with the full leap-year rule: Feb 29 is valid in
/// 2024 and 2000 (÷400) but not 2023 or 1900 (÷100, not ÷400); April has only 30 days; an
/// out-of-range month is rejected.
#[test]
fn test_checkdate_validates_gregorian_dates() {
    let out = compile_and_run(
        r#"<?php
$r = (checkdate(2, 29, 2024) ? "1" : "0")
   . (checkdate(2, 29, 2023) ? "1" : "0")
   . (checkdate(2, 29, 2000) ? "1" : "0")
   . (checkdate(2, 29, 1900) ? "1" : "0")
   . (checkdate(4, 31, 2024) ? "1" : "0")
   . (checkdate(12, 31, 2024) ? "1" : "0")
   . (checkdate(13, 1, 2024) ? "1" : "0");
echo $r;
"#,
    );
    assert_eq!(out, "1010010");
}

/// Verifies getdate() returns PHP's associative array (string keys plus the integer key 0)
/// decomposing a timestamp: 2024-07-01 12:00 UTC is a Monday in July, day-of-year 182.
#[test]
fn test_getdate_decomposes_timestamp() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("UTC");
$g = getdate(1719835200);
echo $g["seconds"], "|", $g["minutes"], "|", $g["hours"], "|", $g["mday"], "|", $g["wday"], "|",
     $g["mon"], "|", $g["year"], "|", $g["yday"], "|", $g["weekday"], "|", $g["month"], "|",
     $g[0], "|", count($g);
"#,
    );
    assert_eq!(out, "0|0|12|1|1|7|2024|182|Monday|July|1719835200|11");
}

/// Verifies gettimeofday() returns PHP's `[sec, usec, minuteswest, dsttime]` array (4 keys; sec and
/// usec in range) and that gettimeofday(true) returns a float. Asserted against the UTC zone where
/// `minuteswest`/`dsttime` are always 0 (UTC has no DST), so the test is season-independent; the
/// current-time components are checked by range, not exact value.
#[test]
fn test_gettimeofday_array_and_float() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("UTC");
$tv = gettimeofday();
$ok = ($tv["sec"] > 1700000000) && ($tv["usec"] >= 0) && ($tv["usec"] < 1000000)
    && ($tv["minuteswest"] === 0) && ($tv["dsttime"] === 0) && (count($tv) === 4);
echo $ok ? "array_ok" : "array_bad";
echo "|", ((gettimeofday(true) > 1700000000.0) ? "float_ok" : "float_bad");
"#,
    );
    assert_eq!(out, "array_ok|float_ok");
}

/// Verifies strftime()/gmstrftime() translate `%`-specifiers to formatted output: 1:1 mappings,
/// composites (`%T`), the computed day-of-year (`%j`) and century (`%C`), surrounding literal text
/// (letters are escaped so `date()` keeps them literal), and `%%`. 2024-06-15 12:00:00 UTC is a
/// Saturday, day-of-year 167; an explicit timestamp keeps the result deterministic.
#[test]
fn test_strftime_specifiers() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("UTC");
$ts = 1718452800;
echo strftime("%Y-%m-%d %H:%M:%S", $ts), "|", strftime("%A %B %d", $ts), "|", strftime("%j", $ts),
     "|", strftime("Day %d", $ts), "|", strftime("%C", $ts), "|", strftime("100%%", $ts),
     "|", gmstrftime("%T", $ts);
"#,
    );
    assert_eq!(out, "2024-06-15 12:00:00|Saturday June 15|167|Day 15|20|100%|12:00:00");
}

/// Verifies idate() returns the integer value of a single date() specifier (not a string), so the
/// result is usable directly in arithmetic. 2024-06-15 12:00:00 UTC: year 2024, month 6, day 15,
/// hour 12, the raw timestamp for `U`, ISO weekday 6 (Saturday), and leap-year flag 1.
#[test]
fn test_idate_integer_specifiers() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("UTC");
$ts = 1718452800;
echo idate("Y", $ts), "|", idate("m", $ts), "|", idate("d", $ts), "|", idate("H", $ts), "|",
     idate("U", $ts), "|", idate("N", $ts), "|", idate("L", $ts), "|", (idate("Y", $ts) + 1);
"#,
    );
    assert_eq!(out, "2024|6|15|12|1718452800|6|1|2025");
}

/// Verifies localtime() returns the raw struct-tm fields: numeric-indexed by default (tm_mon is
/// 0-based, tm_year is years-since-1900), or tm_*-keyed when the second argument is true.
#[test]
fn test_localtime_numeric_and_associative() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("UTC");
$n = localtime(1719835200);
$a = localtime(1719835200, true);
echo $n[2], ",", $n[4], ",", $n[5], ",", $n[6], "|",
     $a["tm_hour"], ",", $a["tm_mon"], ",", $a["tm_year"], ",", $a["tm_wday"], "|", count($n);
"#,
    );
    assert_eq!(out, "12,6,124,1|12,6,124,1|9");
}

/// Verifies the scalar-returning date builtins work as first-class callables
/// (`checkdate(...)`, `gmmktime(...)`, `strtotime(...)`) — i.e. they are accepted by
/// `first_class_callable_builtin_sig` and dispatch correctly when stored and invoked.
#[test]
fn test_first_class_callable_date_scalars() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("UTC");
$cd = checkdate(...);
$gm = gmmktime(...);
$st = strtotime(...);
echo $cd(2, 29, 2024) ? "y" : "n";
echo $cd(2, 29, 2023) ? "y" : "n";
echo "|", $gm(0, 0, 0, 1, 1, 2000);
echo "|", $st("@946684800");
"#,
    );
    assert_eq!(out, "yn|946684800|946684800");
}

/// Verifies the array-returning date builtins work as first-class callables
/// (`getdate(...)`, `localtime(...)`), preserving PHP's associative key names through
/// the stored-callable invocation path.
#[test]
fn test_first_class_callable_date_arrays() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("UTC");
$gd = getdate(...);
$lt = localtime(...);
$g = $gd(1719835200);
$a = $lt(1719835200, true);
echo $g["year"], "-", $g["mon"], "-", $g["mday"], "|", $a["tm_year"], ",", $a["tm_mon"];
"#,
    );
    assert_eq!(out, "2024-7-1|124,6");
}

/// Verifies `hrtime(...)` works as a first-class callable and that the stored callable
/// returns a positive nanosecond count when invoked with `$as_number = true`.
#[test]
fn test_first_class_callable_hrtime() {
    let out = compile_and_run(
        r#"<?php
$hr = hrtime(...);
echo $hr(true) > 0 ? "y" : "n";
"#,
    );
    assert_eq!(out, "y");
}

/// Verifies hrtime() returns a [seconds, nanoseconds] pair by default and the total nanoseconds as
/// an int when $as_number is true, with a monotonic (non-decreasing) clock.
#[test]
fn test_hrtime_array_and_nanoseconds() {
    let out = compile_and_run(
        r#"<?php
$a = hrtime();
$b = hrtime(true);
$c = hrtime(true);
echo count($a), "|",
     ((is_int($a[0]) && is_int($a[1]) && $a[1] >= 0 && $a[1] < 1000000000) ? "1" : "0"), "|",
     (is_int($b) ? "1" : "0"), "|",
     (($b > 0 && $c >= $b) ? "1" : "0");
"#,
    );
    assert_eq!(out, "2|1|1|1");
}

/// Verifies `function_exists()` returns `true` for the procedural date/time aliases that the
/// name resolver rewrites into OOP/built-in expressions (e.g. `date_create`, `idate`,
/// `gmstrftime`). PHP's introspection must recognize the same surface that the resolver sees.
#[test]
fn test_function_exists_date_procedural_aliases() {
    let out = compile_and_run(
        r#"<?php
echo function_exists("date_create") ? "1" : "0";
echo function_exists("date_create_immutable") ? "1" : "0";
echo function_exists("date_create_from_format") ? "1" : "0";
echo function_exists("date_create_immutable_from_format") ? "1" : "0";
echo function_exists("date_diff") ? "1" : "0";
echo function_exists("date_format") ? "1" : "0";
echo function_exists("date_add") ? "1" : "0";
echo function_exists("date_sub") ? "1" : "0";
echo function_exists("date_modify") ? "1" : "0";
echo function_exists("date_timestamp_get") ? "1" : "0";
echo function_exists("date_timestamp_set") ? "1" : "0";
echo function_exists("date_timezone_get") ? "1" : "0";
echo function_exists("date_timezone_set") ? "1" : "0";
echo function_exists("date_offset_get") ? "1" : "0";
echo function_exists("date_date_set") ? "1" : "0";
echo function_exists("date_isodate_set") ? "1" : "0";
echo function_exists("date_time_set") ? "1" : "0";
echo function_exists("date_interval_format") ? "1" : "0";
echo function_exists("date_interval_create_from_date_string") ? "1" : "0";
echo function_exists("date_parse") ? "1" : "0";
echo function_exists("date_parse_from_format") ? "1" : "0";
echo function_exists("date_get_last_errors") ? "1" : "0";
echo function_exists("idate") ? "1" : "0";
echo function_exists("gettimeofday") ? "1" : "0";
echo function_exists("strftime") ? "1" : "0";
echo function_exists("gmstrftime") ? "1" : "0";
echo function_exists("timezone_open") ? "1" : "0";
echo function_exists("timezone_identifiers_list") ? "1" : "0";
echo function_exists("timezone_name_get") ? "1" : "0";
echo function_exists("timezone_offset_get") ? "1" : "0";
echo function_exists("Date_Create") ? "1" : "0";
echo function_exists("IDATE") ? "1" : "0";
echo function_exists("\\date_create") ? "1" : "0";
echo function_exists("\\foo\\bar\\idate") ? "1" : "0";
echo function_exists("does_not_exist_alias_xyz") ? "1" : "0";
"#,
    );
    assert_eq!(out, "1".repeat(34) + "0");
}

/// Verifies `mktime`/`gmmktime` accept PHP 8.0+'s optional arguments: omitted trailing components
/// default to the current time's value (year for a 5-arg call; everything but the hour for a 1-arg
/// call), while the 6-arg form stays deterministic. `function_exists` still recognizes both as
/// procedural aliases (they desugar to `__elephc_mktime_raw`/`__elephc_gmmktime_raw`).
#[test]
fn test_mktime_optional_args() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("UTC");
// 6-arg form: unchanged deterministic timestamp.
echo date("Y-m-d H:i:s", mktime(12, 30, 45, 6, 15, 2024)), "|";
// 5-arg form (year omitted): month/day kept, year defaults to the current year.
$ts = mktime(0, 0, 0, 3, 15);
echo date("m-d", $ts), "|", (idate("Y", $ts) === idate("Y") ? "same-year" : "diff"), "|";
// 1-arg form (only hour): hour kept, the rest default to the current time.
echo idate("G", mktime(12)) === 12 ? "h12" : "x", "|";
// function_exists still sees the procedural aliases.
echo function_exists("mktime") ? "1" : "0", function_exists("gmmktime") ? "1" : "0";
"#,
    );
    assert_eq!(out, "2024-06-15 12:30:45|03-15|same-year|h12|11");
}
