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
        r#"{"floats":[1.5,2.25],"bools":[true,false],"objects":[null]}"#
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
    let out = compile_and_run(r#"<?php echo json_decode(" true ");"#);
    assert_eq!(out, "true");
}

#[test]
fn test_json_decode_null_literal() {
    let out = compile_and_run(r#"<?php echo json_decode("\nnull\t");"#);
    assert_eq!(out, "null");
}

#[test]
fn test_json_decode_array_passthrough() {
    let out = compile_and_run(r#"<?php echo json_decode(" [1, 2, 3] ");"#);
    assert_eq!(out, "[1, 2, 3]");
}

#[test]
fn test_json_decode_assoc_passthrough() {
    let out = compile_and_run(r#"<?php echo json_decode(" {\"a\": 1} ");"#);
    assert_eq!(out, r#"{"a": 1}"#);
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

