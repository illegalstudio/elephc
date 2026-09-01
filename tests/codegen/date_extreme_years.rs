//! Purpose:
//! Regression coverage for php-src DateTime and mktime behavior at extreme and historical years.
//!
//! Called from:
//! - `cargo test --test codegen_tests date_extreme_years` through Rust's test harness.
//!
//! Key details:
//! - Fixtures mirror `big_year.phpt`, `gh18422.phpt`, and the historical-offset rows of
//!   `mktime-3-64bit.phpt` from the frozen php-src date suite.

use crate::support::*;

/// Preserves the full 64-bit civil year through mktime and RFC 2822 formatting.
#[test]
fn test_big_year_phpt_regression() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("America/Toronto");
$timestamp = mktime(0, 0, 0, 1, 1, 2922770265);
var_dump(date("r", $timestamp));
echo "OK\n";
"#,
    );
    assert_eq!(
        out,
        "string(37) \"Sun, 01 Jan 2922770265 00:00:00 -0500\"\nOK\n"
    );
}

/// Keeps the signed-minimum ISO week year as a civil field instead of replacing it with year 2000.
#[test]
fn test_gh18422_minimum_iso_year_regression() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("UTC");
$date = date_create("2006-12-12");
date_isodate_set($date, PHP_INT_MIN, 1, 1);
echo $date->format("Y"), "\n", $date->format("x"), "\n", $date->format("X"), "\n";
echo date_create("2024-06-15")->format("Y"), "\n";
echo date_create("-0042-01-01")->format("Y"), "\n";
"#,
    );
    assert_eq!(
        out,
        "-9223372036854775808\n-9223372036854775808\n-9223372036854775808\n2024\n-0042\n"
    );
}

/// Uses timelib's historical local-mean-time offsets without shifting the requested wall clock.
#[test]
fn test_mktime_64bit_historical_wall_clock_regression() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("America/Toronto");
echo date("Y-m-d\\TH:i:sO", mktime(1, 1, 1, 1, 1, 101)), "\n";
date_default_timezone_set("Europe/Oslo");
echo date("Y-m-d\\TH:i:sO", mktime(1, 1, 1, 1, 1, 101)), "\n";
"#,
    );
    assert_eq!(
        out,
        "0101-01-01T01:01:01-0517\n0101-01-01T01:01:01+0043\n"
    );
}
