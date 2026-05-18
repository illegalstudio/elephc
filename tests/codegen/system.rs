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

#[test]
fn test_date_year() {
    let out = compile_and_run("<?php echo date(\"Y\", 1700000000);");
    assert_eq!(out, "2023");
}

#[test]
fn test_date_full_format() {
    let out = compile_and_run("<?php echo date(\"Y-m-d\", 1700000000);");
    assert_eq!(out, "2023-11-14");
}

#[test]
fn test_date_time_format() {
    let out = compile_and_run("<?php echo date(\"H:i:s\", 1700000000);");
    // The exact output depends on the timezone, but it should have the format HH:MM:SS
    let out_trimmed = out.trim();
    assert_eq!(out_trimmed.len(), 8);
    assert_eq!(&out_trimmed[2..3], ":");
    assert_eq!(&out_trimmed[5..6], ":");
}

#[test]
fn test_date_day_no_padding() {
    let out = compile_and_run("<?php echo date(\"j\", 1700000000);");
    let val: i32 = out.trim().parse().unwrap();
    assert!(val >= 1 && val <= 31);
}

#[test]
fn test_date_am_pm() {
    let out = compile_and_run("<?php echo date(\"A\", 1700000000);");
    assert!(out == "AM" || out == "PM");
}

#[test]
fn test_date_am_pm_lower() {
    let out = compile_and_run("<?php echo date(\"a\", 1700000000);");
    assert!(out == "am" || out == "pm");
}

#[test]
fn test_date_unix_timestamp() {
    let out = compile_and_run("<?php echo date(\"U\", 1700000000);");
    assert_eq!(out, "1700000000");
}

#[test]
fn test_date_short_day() {
    let out = compile_and_run("<?php echo date(\"D\", 1700000000);");
    let valid_days = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
    assert!(valid_days.contains(&out.as_str()), "Got: {}", out);
}

#[test]
fn test_date_short_month() {
    let out = compile_and_run("<?php echo date(\"M\", 1700000000);");
    assert_eq!(out, "Nov");
}

#[test]
fn test_date_iso_day_of_week() {
    let out = compile_and_run("<?php echo date(\"N\", 1700000000);");
    let val: i32 = out.trim().parse().unwrap();
    assert!(val >= 1 && val <= 7);
}

#[test]
fn test_date_12_hour() {
    let out = compile_and_run("<?php echo date(\"g\", 1700000000);");
    let val: i32 = out.trim().parse().unwrap();
    assert!(val >= 1 && val <= 12);
}

#[test]
fn test_date_literal_text() {
    let out = compile_and_run("<?php echo date(\"Y/m/d\", 1700000000);");
    assert_eq!(out, "2023/11/14");
}

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

#[test]
fn test_strtotime_trims_ascii_whitespace() {
    let out = compile_and_run(
        "<?php $ts = strtotime(\"\\n\\t today \\n\"); echo date(\"H:i:s\", $ts);",
    );
    assert_eq!(out, "00:00:00");
}

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

#[test]
fn test_date_current_time() {
    // date() with no timestamp should use current time
    let out = compile_and_run(
        "<?php $y = date(\"Y\"); $val = intval($y); if ($val >= 2024) { echo \"ok\"; }",
    );
    assert_eq!(out, "ok");
}

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

#[test]
fn test_date_full_month_name() {
    let out = compile_and_run("<?php echo date(\"F\", 1700000000);");
    assert_eq!(out, "November");
}

#[test]
fn test_date_epoch_zero_timestamp() {
    // Regression test for GitHub issue #9: date("Y-m-d", 0) should format Unix epoch,
    // not return the current date. Timestamp 0 = 1970-01-01 00:00:00 UTC.
    let out = compile_and_run("<?php echo date(\"Y\", 0);");
    assert_eq!(out, "1970");
}

// --- JSON functions ---

#[test]
fn test_json_encode_int() {
    let out = compile_and_run("<?php echo json_encode(42);");
    assert_eq!(out, "42");
}

#[test]
fn test_json_encode_string() {
    let out = compile_and_run(r#"<?php echo json_encode("hello");"#);
    assert_eq!(out, r#""hello""#);
}

#[test]
fn test_json_encode_string_with_escaping() {
    let out = compile_and_run("<?php echo json_encode(\"hello\\nworld\");");
    assert_eq!(out, r#""hello\nworld""#);
}

#[test]
fn test_json_encode_string_with_quotes() {
    let out = compile_and_run(r#"<?php echo json_encode("say \"hi\"");"#);
    assert_eq!(out, r#""say \"hi\"""#);
}

#[test]
fn test_json_encode_bool_true() {
    let out = compile_and_run("<?php echo json_encode(true);");
    assert_eq!(out, "true");
}

#[test]
fn test_json_encode_bool_false() {
    let out = compile_and_run("<?php echo json_encode(false);");
    assert_eq!(out, "false");
}

#[test]
fn test_json_encode_null() {
    let out = compile_and_run("<?php echo json_encode(null);");
    assert_eq!(out, "null");
}

#[test]
fn test_json_encode_int_array() {
    let out = compile_and_run("<?php echo json_encode([1, 2, 3]);");
    assert_eq!(out, "[1,2,3]");
}

#[test]
fn test_json_encode_string_array() {
    let out = compile_and_run(r#"<?php echo json_encode(["a", "b", "c"]);"#);
    assert_eq!(out, r#"["a","b","c"]"#);
}

#[test]
fn test_json_encode_string_array_with_escaping() {
    let out = compile_and_run("<?php echo json_encode([\"a\\n\", \"b\\\"\", \"c\\\\\"]);");
    assert_eq!(out, "[\"a\\n\",\"b\\\"\",\"c\\\\\"]");
}

#[test]
fn test_json_encode_single_element_array() {
    let out = compile_and_run("<?php $arr = [42]; echo json_encode($arr);");
    assert_eq!(out, "[42]");
}

#[test]
fn test_json_encode_assoc() {
    let out = compile_and_run(r#"<?php echo json_encode(["name" => "Alice", "age" => "30"]);"#);
    assert_eq!(out, r#"{"name":"Alice","age":"30"}"#, "Got: {}", out);
}

#[test]
fn test_json_encode_assoc_integer_keys() {
    let out = compile_and_run(r#"<?php echo json_encode([1 => "one", "02" => "two"]);"#);
    assert_eq!(out, r#"{"1":"one","02":"two"}"#);
}

#[test]
fn test_json_encode_assoc_mixed_values() {
    let out = compile_and_run(
        r#"<?php echo json_encode(["id" => 7, "name" => "Alice", "ok" => true, "note" => null]);"#,
    );
    assert_eq!(out, r#"{"id":7,"name":"Alice","ok":true,"note":null}"#);
}

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

#[test]
fn test_json_encode_float() {
    let out = compile_and_run("<?php echo json_encode(3.14);");
    assert!(out.starts_with("3.14"), "Got: {}", out);
}

#[test]
fn test_json_last_error() {
    let out = compile_and_run("<?php echo json_last_error();");
    assert_eq!(out, "0");
}

#[test]
fn test_json_decode_string() {
    let out = compile_and_run(r#"<?php echo json_decode("\"hello\"");"#);
    assert_eq!(out, "hello");
}

#[test]
fn test_json_decode_number() {
    let out = compile_and_run(r#"<?php echo json_decode("42");"#);
    assert_eq!(out, "42");
}

#[test]
fn test_json_decode_escaped() {
    let out = compile_and_run(r#"<?php $s = json_decode("\"hello\\nworld\""); echo strlen($s);"#);
    assert_eq!(out, "11"); // "hello" + newline + "world" = 11 chars
}

#[test]
fn test_json_decode_escaped_quote_and_backslash() {
    let out = compile_and_run(r#"<?php echo json_decode("\"a\\\"b\\\\c\"");"#);
    assert_eq!(out, "a\"b\\c");
}

#[test]
fn test_json_decode_trimmed_string() {
    let out = compile_and_run(r#"<?php echo json_decode("   \"hello\"   ");"#);
    assert_eq!(out, "hello");
}

#[test]
fn test_json_decode_true_literal() {
    // PHP-faithful: json_decode("true") returns Mixed(bool=true), and
    // `echo true` prints "1" (PHP's bool→string coercion rule).
    let out = compile_and_run(r#"<?php echo json_decode(" true ");"#);
    assert_eq!(out, "1");
}

#[test]
fn test_json_decode_null_literal() {
    // PHP-faithful: json_decode("null") returns Mixed(null), and `echo null`
    // prints nothing (the empty string).
    let out = compile_and_run(r#"<?php echo json_decode("\nnull\t");"#);
    assert_eq!(out, "");
}

#[test]
fn test_json_decode_array_round_trip() {
    // json_decode now returns a structural Mixed(array) for non-empty
    // arrays; round-trip through json_encode produces the canonical
    // compact form (whitespace dropped, scalars re-encoded).
    let out = compile_and_run(r#"<?php echo json_encode(json_decode(" [1, 2, 3] "));"#);
    assert_eq!(out, "[1,2,3]");
}

#[test]
fn test_json_decode_assoc_round_trip() {
    // json_decode now returns a structural Mixed(assoc) for non-empty
    // objects too; round-trip through json_encode produces the canonical
    // compact form.
    let out = compile_and_run(r#"<?php echo json_encode(json_decode(" {\"a\": 1} "));"#);
    assert_eq!(out, r#"{"a":1}"#);
}

#[test]
fn test_json_decode_escaped_solidus() {
    let out = compile_and_run(r#"<?php echo json_decode("\"https:\\/\\/example.com\"");"#);
    assert_eq!(out, "https://example.com");
}

#[test]
fn test_json_decode_unicode_bmp_latin1() {
    let out = compile_and_run(r#"<?php echo json_decode("\"caf\u00e9\"");"#);
    assert_eq!(out, "café");
}

#[test]
fn test_json_decode_unicode_bmp_multibyte() {
    let out = compile_and_run(r#"<?php echo json_decode("\"\u4f60\u597d\"");"#);
    assert_eq!(out, "你好");
}

#[test]
fn test_json_decode_unicode_surrogate_pair() {
    let out = compile_and_run(r#"<?php $s = json_decode("\"\ud83d\ude00\""); echo strlen($s);"#);
    assert_eq!(out, "4");
}

// --- Regex functions ---

#[test]
fn test_preg_match_simple() {
    let out = compile_and_run(r#"<?php echo preg_match("/hello/", "hello world");"#);
    assert_eq!(out, "1");
}

#[test]
fn test_preg_match_no_match() {
    let out = compile_and_run(r#"<?php echo preg_match("/xyz/", "hello world");"#);
    assert_eq!(out, "0");
}

#[test]
fn test_preg_match_case_insensitive() {
    let out = compile_and_run(r#"<?php echo preg_match("/HELLO/i", "hello world");"#);
    assert_eq!(out, "1");
}

#[test]
fn test_preg_match_pattern() {
    let out = compile_and_run(r#"<?php echo preg_match("/[0-9]+/", "abc123def");"#);
    assert_eq!(out, "1");
}

#[test]
fn test_preg_match_no_digits() {
    let out = compile_and_run(r#"<?php echo preg_match("/[0-9]+/", "abcdef");"#);
    assert_eq!(out, "0");
}

#[test]
fn test_preg_match_all_count() {
    let out = compile_and_run(r#"<?php echo preg_match_all("/[0-9]+/", "a1b2c3");"#);
    assert_eq!(out, "3");
}

#[test]
fn test_preg_match_all_no_matches() {
    let out = compile_and_run(r#"<?php echo preg_match_all("/[0-9]+/", "abcdef");"#);
    assert_eq!(out, "0");
}

#[test]
fn test_preg_replace_simple() {
    let out = compile_and_run(r#"<?php echo preg_replace("/world/", "PHP", "hello world");"#);
    assert_eq!(out, "hello PHP");
}

#[test]
fn test_preg_replace_pattern() {
    let out = compile_and_run(r#"<?php echo preg_replace("/[0-9]+/", "X", "a1b2c3");"#);
    assert_eq!(out, "aXbXcX");
}

#[test]
fn test_preg_replace_no_match() {
    let out = compile_and_run(r#"<?php echo preg_replace("/xyz/", "ABC", "hello world");"#);
    assert_eq!(out, "hello world");
}

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

#[test]
fn test_preg_replace_case_insensitive() {
    let out = compile_and_run(r#"<?php echo preg_replace("/WORLD/i", "PHP", "hello World");"#);
    assert_eq!(out, "hello PHP");
}

#[test]
fn test_preg_replace_dollar_backreferences() {
    let out = compile_and_run(
        r#"<?php echo preg_replace("/([a-z]+) ([a-z]+)/", '$2, $1', "hello world");"#,
    );
    assert_eq!(out, "world, hello");
}

#[test]
fn test_preg_replace_backslash_backreferences() {
    let out = compile_and_run(r#"<?php echo preg_replace("/([0-9]+)-([0-9]+)/", "\\2/\\1", "12-34");"#);
    assert_eq!(out, "34/12");
}

#[test]
fn test_preg_replace_unmatched_capture_backreference_is_empty() {
    let out = compile_and_run(r#"<?php echo preg_replace("/(a)(b)?/", '[$1][$2]', "a");"#);
    assert_eq!(out, "[a][]");
}
// is_callable() — compile-time decisions for string literals (catalog
// lookup) and Callable-typed values (closures + first-class callables).

#[test]
fn test_is_callable_known_builtin_returns_true() {
    let out = compile_and_run(r#"<?php echo is_callable("json_encode") ? "y" : "n";"#);
    assert_eq!(out, "y");
}

#[test]
fn test_is_callable_unknown_string_returns_false() {
    let out = compile_and_run(r#"<?php echo is_callable("nope_xyz_no_such_fn") ? "y" : "n";"#);
    assert_eq!(out, "n");
}

#[test]
fn test_is_callable_case_insensitive_builtin() {
    let out = compile_and_run(r#"<?php echo is_callable("Json_Encode") ? "y" : "n";"#);
    assert_eq!(out, "y");
}

#[test]
fn test_is_callable_uppercase_builtin() {
    let out = compile_and_run(r#"<?php echo is_callable("JSON_DECODE") ? "y" : "n";"#);
    assert_eq!(out, "y");
}

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

#[test]
fn test_is_callable_int_returns_false() {
    let out = compile_and_run(r#"<?php echo is_callable(42) ? "y" : "n";"#);
    assert_eq!(out, "n");
}

#[test]
fn test_is_callable_bool_returns_false() {
    let out = compile_and_run(r#"<?php echo is_callable(true) ? "y" : "n";"#);
    assert_eq!(out, "n");
}

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
