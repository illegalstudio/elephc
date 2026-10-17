//! Purpose:
//! Checks empty-source, string-token and validator JSON diagnostic regressions.
//!
//! Called from:
//! - The JSON codegen suite through Rust's test harness.
//!
//! Key details:
//! - Every case runs with both optimizer modes and normal/heap-debug runtimes.
//! - PHP 8.6 compares complete coordinates; older oracles compare compatible base messages.

use super::*;
use super::error_location::{php_cli, php_json_error_has_locations, php_json_version, scan_counter_harness, strip_location_suffix};

/// Checks PHP base-message parity and native diagnostics under all fixture modes.
pub(super) fn assert_json_diagnostics(source: &str, expected: &str) {
    assert_json_diagnostics_since(source, expected, 80300);
}

/// Runs every native fixture mode, selecting the live oracle only from its reference version.
pub(super) fn assert_json_diagnostics_since(source: &str, expected: &str, minimum_php_version: u32) {
    if std::env::var_os("ELEPHC_PHP_CHECK").is_some() && php_json_version() >= minimum_php_version {
        let oracle = php_cli(source.strip_prefix("<?php").unwrap());
        let expected = if php_json_error_has_locations() {
            expected.trim_end().to_owned()
        } else {
            expected.lines().map(strip_location_suffix).collect::<Vec<_>>().join("\n")
        };
        assert_eq!(oracle.trim_end(), expected, "PHP JSON diagnostics differ");
    }
    for ir_opt_off in [false, true] {
        let run = || {
            assert_eq!(compile_and_run(source), expected);
            let output = compile_and_run_with_heap_debug(source);
            assert!(output.success, "{}", output.stderr);
            assert_eq!(output.stdout, expected, "{}", output.stderr);
            assert!(output.stderr.contains("HEAP DEBUG:"), "heap debug did not run");
        };
        if ir_opt_off {
            without_ir_opt(run);
        } else {
            run();
        }
    }
}

/// Empty input clears prior coordinates and never suffixes an ordinary or thrown error.
#[test]
fn test_json_diagnostics_empty_source_has_no_location() {
    let source = r#"<?php
json_decode("[{");
json_decode("");
echo json_last_error(), "|", json_last_error_msg(), "\n";
try {
    json_decode("", null, 512, JSON_THROW_ON_ERROR);
} catch (JsonException $e) {
    echo get_class($e), "|", $e->getCode(), "|", $e->getMessage(), "\n";
}
json_validate("[{");
json_validate("");
echo json_last_error(), "|", json_last_error_msg(), "\n";
"#;
    assert_json_diagnostics(source, "4|Syntax error\nJsonException|4|Syntax error\n4|Syntax error\n");
}

/// String errors in values and keys use the opening quote and Unicode character columns.
#[test]
fn test_json_diagnostics_string_errors_use_token_opening() {
    let source = r#"<?php
$cases = [
    "[\"" . chr(0) . "abcd\"]",
    "[\"a" . chr(0x80) . "b\"]",
    '["a\uD83D"]',
    '"a\uD83D"',
    '{"bad\uD83D":0}',
    "[0,\n  \"ab" . chr(0) . "cd\"]",
    "[\"é\", \"bad" . chr(0x80) . "\"]"
];
foreach ($cases as $json) {
    json_decode($json);
    echo json_last_error(), "|", json_last_error_msg(), "\n";
    json_validate($json);
    echo json_last_error(), "|", json_last_error_msg(), "\n";
    try {
        json_decode($json, null, 512, JSON_THROW_ON_ERROR);
    } catch (JsonException $e) {
        echo $e->getCode(), "|", $e->getMessage(), "\n";
    }
}
"#;
    let mut expected = String::new();
    for (code, message, location) in [
        (3, "Control character error, possibly incorrectly encoded", "1:2"),
        (5, "Malformed UTF-8 characters, possibly incorrectly encoded", "1:2"),
        (10, "Single unpaired UTF-16 surrogate in unicode escape", "1:2"),
        (10, "Single unpaired UTF-16 surrogate in unicode escape", "1:1"),
        (10, "Single unpaired UTF-16 surrogate in unicode escape", "1:2"),
        (3, "Control character error, possibly incorrectly encoded", "2:3"),
        (5, "Malformed UTF-8 characters, possibly incorrectly encoded", "1:7"),
    ] {
        for _ in 0..3 {
            expected.push_str(&format!("{code}|{message} near location {location}\n"));
        }
    }
    assert_json_diagnostics(source, &expected);
}

/// Empty sources clear the internal numeric coordinates after a previous failure.
#[test]
fn test_json_diagnostics_empty_source_coordinates_are_zero() {
    let harness = scan_counter_harness()
        .replace("_json_error_scan_count", "_json_error_line")
        .replace("_json_error_scan_bytes", "_json_error_column");
    for source in [
        r#"<?php json_decode("[1,\n 2,]"); json_decode("");"#,
        r#"<?php json_validate("-"); json_validate("");"#,
        r#"<?php
json_decode("[1,\n 2,]");
try { json_decode("", null, 512, JSON_THROW_ON_ERROR); }
catch (JsonException $e) { }
"#,
    ] {
        for ir_opt_off in [false, true] {
            let run = || compile_harness_and_run_with_heap_debug(source, 8_388_608, &harness);
            let output = if ir_opt_off { without_ir_opt(run) } else { run() };
            assert_eq!(output, "0\n0\n", "ir-opt off={ir_opt_off}: {source}");
        }
    }
}

/// Validate publishes a bounded source, preserves numeric coordinates and resets success.
#[test]
fn test_json_diagnostics_validate_publishes_its_source() {
    let source = r#"<?php
function invalid_json() { return "-"; }
json_validate(invalid_json());
$replacement = str_repeat("Z", 64);
echo json_last_error(), "|", json_last_error_msg(), "\n";
json_validate('{"a":{"b":1}}', 1);
echo json_last_error(), "|", json_last_error_msg(), "\n";
json_validate("");
echo json_last_error(), "|", json_last_error_msg(), "\n";
json_validate("true");
echo json_last_error(), "|", json_last_error_msg(), "\n";
"#;
    assert_json_diagnostics(source, "4|Syntax error near location 1:1\n1|Maximum stack depth exceeded near location 1:1\n4|Syntax error\n0|No error\n");
}
