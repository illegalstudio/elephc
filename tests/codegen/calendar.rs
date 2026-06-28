//! Purpose:
//! Codegen tests for the PHP `ext/calendar` extension functions (Julian-Day conversions for the
//! Gregorian, Julian, French Republican and Jewish calendars, Easter, day/month names, and the
//! `cal_*` dispatchers).
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Every expected value is byte-identical to PHP (cross-checked against the `ext/calendar`
//!   builtins under a UTC default timezone). The algorithms are pure integer Serial-Day-Number math.

use crate::support::*;

/// Verifies the Gregorian and Julian Julian-Day conversions and their round trips, including the
/// `m/d/y` string format PHP returns from `jdtogregorian()`/`jdtojulian()`.
#[test]
fn test_calendar_gregorian_julian() {
    let out = compile_and_run(
        r#"<?php
echo gregoriantojd(10, 9, 1995), ",", gregoriantojd(1, 1, 2000), ",", gregoriantojd(2, 29, 2000), "|";
echo jdtogregorian(2450001), ",", jdtogregorian(gregoriantojd(1, 1, 2000)), ",", jdtogregorian(0), "|";
echo juliantojd(10, 9, 1995), ",", jdtojulian(2451545);
"#,
    );
    assert_eq!(out, "2450000,2451545,2451604|10/10/1995,1/1/2000,0/0/0|2450013,12/19/1999");
}

/// Verifies the French Republican and Jewish calendar conversions, including out-of-range French
/// dates (which yield `0/0/0`) and Jewish round trips through `jewishtojd`/`jdtojewish`.
#[test]
fn test_calendar_french_jewish() {
    let out = compile_and_run(
        r#"<?php
echo frenchtojd(1, 1, 1), ",", jdtofrench(2375840), ",", jdtofrench(0), "|";
echo jewishtojd(7, 1, 5784), ",", jdtojewish(2451545), ",", jdtojewish(gregoriantojd(9, 16, 2023));
"#,
    );
    assert_eq!(out, "2375840,1/1/1,0/0/0|2460381,4/23/5760,1/1/5784");
}

/// Verifies `easter_days()` (Gregorian and `CAL_EASTER_ALWAYS_JULIAN`) and `easter_date()` as a
/// UTC midnight timestamp, plus `jddayofweek()` across its three return modes.
#[test]
fn test_calendar_easter_and_dow() {
    let out = compile_and_run(
        r#"<?php
echo easter_days(2024), ",", easter_days(2000), ",", easter_days(1999), ",", easter_days(1750, CAL_EASTER_ALWAYS_JULIAN), "|";
echo gmdate("Y-m-d", easter_date(2024)), "|";
echo jddayofweek(2451545, 0), ",", jddayofweek(2451545, CAL_DOW_LONG), ",", jddayofweek(2451545, CAL_DOW_SHORT);
"#,
    );
    assert_eq!(out, "10,33,14,25|2024-03-31|6,Saturday,Sat");
}

/// Verifies `jdmonthname()` across all calendar modes, the Unix↔JD conversions, and
/// `cal_days_in_month()` for the Gregorian, Jewish, and French calendars.
#[test]
fn test_calendar_monthname_unix_days() {
    let out = compile_and_run(
        r#"<?php
echo jdmonthname(2451545, CAL_MONTH_GREGORIAN_SHORT), ",", jdmonthname(2451545, CAL_MONTH_GREGORIAN_LONG), ",";
echo jdmonthname(gregoriantojd(9, 16, 2023), CAL_MONTH_JEWISH), ",", jdmonthname(2375840, CAL_MONTH_FRENCH), "|";
echo unixtojd(0), ",", unixtojd(1000000000), ",", jdtounix(2440588), ",", jdtounix(2451545), "|";
echo cal_days_in_month(CAL_GREGORIAN, 2, 2000), ",", cal_days_in_month(CAL_GREGORIAN, 2, 1900), ",";
echo cal_days_in_month(CAL_JEWISH, 1, 5784), ",", cal_days_in_month(CAL_FRENCH, 13, 5);
"#,
    );
    assert_eq!(
        out,
        "Jan,January,Tishri,Vendemiaire|2440588,2452162,0,946684800|29,28,30,5"
    );
}

/// Verifies the `cal_from_jd()` array (date string, day-of-week, day/month names) and
/// `cal_info()` (a single calendar plus the all-calendars form).
#[test]
fn test_calendar_cal_from_jd_and_info() {
    let out = compile_and_run(
        r#"<?php
$ci = cal_from_jd(2451545, CAL_GREGORIAN);
echo $ci["date"], ",", $ci["dow"], ",", $ci["dayname"], ",", $ci["monthname"], ",", $ci["abbrevmonth"], "|";
echo cal_info(0)["calname"], ",", cal_info(CAL_JEWISH)["calname"], ",", count(cal_info());
"#,
    );
    assert_eq!(out, "1/1/2000,6,Saturday,January,Jan|Gregorian,Jewish,4");
}

/// Verifies `cal_to_jd()` dispatches to each calendar identically to the direct `*tojd` functions,
/// and that `function_exists()` recognizes the desugared calendar aliases.
#[test]
fn test_calendar_cal_to_jd_and_function_exists() {
    let out = compile_and_run(
        r#"<?php
echo cal_to_jd(CAL_GREGORIAN, 1, 1, 2000), ",", cal_to_jd(CAL_JULIAN, 1, 1, 2000), ",";
echo cal_to_jd(CAL_JEWISH, 7, 1, 5784), ",", cal_to_jd(CAL_FRENCH, 1, 1, 1), "|";
echo function_exists("gregoriantojd") ? "1" : "0", function_exists("jdtojewish") ? "1" : "0", function_exists("cal_info") ? "1" : "0";
"#,
    );
    assert_eq!(out, "2451545,2451558,2460381,2375840|111");
}
