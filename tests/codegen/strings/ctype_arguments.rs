//! Purpose:
//! Integration tests for the non-string arguments of PHP's `mixed $text` `ctype_*` builtins and
//! for calling them through runtime-selected callables.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Every expectation is the PHP 8.5 output of the same fixture: an int in 0..=255 or
//!   -128..=-1 is classified as one byte, other ints as their decimal string, every other
//!   non-string value is `false`, and each non-string argument raises the deprecation.
//! - Diagnostics are compared after `normalize_diagnostic_text()` removes the volatile location.

use super::*;

/// The deprecation php-src 8.1+ raises for one non-string `ctype_*` argument.
fn ctype_deprecation(function: &str, type_name: &str) -> String {
    format!(
        "Deprecated: {function}(): Argument of type {type_name} will be interpreted as string in the future"
    )
}

/// Verifies every `ctype_*` class over ints at the byte-mapping and decimal-string boundaries.
///
/// 0..=255 is one byte, -128..=-1 is the byte `value + 256`, and the remaining ints answer as
/// their decimal string would: only alnum, digit, graph, print and xdigit accept digits, and
/// only graph and print accept a leading `-`. Each call deprecates its int argument.
#[test]
fn test_ctype_int_arguments_follow_php_byte_rules() {
    let out = compile_and_run_capture(
        r#"<?php
foreach ([0, 9, 31, 48, 65, 97, 127, 128, 255, 256, 1000, -1, -96, -128, -129, PHP_INT_MAX, PHP_INT_MIN] as $i) {
    echo ctype_alnum($i) ? "1" : "0", ctype_alpha($i) ? "1" : "0", ctype_cntrl($i) ? "1" : "0",
        ctype_digit($i) ? "1" : "0", ctype_graph($i) ? "1" : "0", ctype_lower($i) ? "1" : "0",
        ctype_print($i) ? "1" : "0", ctype_punct($i) ? "1" : "0", ctype_space($i) ? "1" : "0",
        ctype_upper($i) ? "1" : "0", ctype_xdigit($i) ? "1" : "0", " ";
}
echo ctype_digit(5) ? "1" : "0", ctype_cntrl(10) ? "1" : "0", \CTYPE_UPPER(65) ? "1" : "0";
"#,
    );
    assert!(out.success, "stderr: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "00100000000 00100000100 00100000000 10011010001 11001010011 11001110001 00100000000 \
         00000000000 00000000000 10011010001 10011010001 00000000000 00000000000 00000000000 \
         00001010000 10011010001 00001010000 011"
    );
    let stderr = normalize_diagnostic_text(&out.stderr);
    let lines: Vec<&str> = stderr.lines().collect();
    assert_eq!(lines.len(), 17 * 11 + 3, "stderr: {stderr}");
    assert_eq!(lines[0], ctype_deprecation("ctype_alnum", "int"));
    assert_eq!(lines[10], ctype_deprecation("ctype_xdigit", "int"));
    assert_eq!(lines[189], ctype_deprecation("ctype_upper", "int"));
}

/// Verifies statically typed non-string arguments deprecate with php-src's type names and fail.
///
/// Objects are named by their class (a closure as `Closure`), and a resource keeps its own
/// name although it shares the int storage representation.
#[test]
fn test_ctype_static_non_string_arguments_deprecate_and_fail() {
    let out = compile_and_run_capture(
        r#"<?php
class Token {}
$handle = fopen("php://memory", "r");
echo ctype_digit(true) ? "1" : "0";
echo ctype_print(1.5) ? "1" : "0";
echo ctype_space(null) ? "1" : "0";
echo ctype_alpha(["a"]) ? "1" : "0";
echo ctype_alnum(new Token()) ? "1" : "0";
echo ctype_graph($handle) ? "1" : "0";
echo ctype_lower(fn() => "a") ? "1" : "0";
"#,
    );
    assert!(out.success, "stderr: {}", out.stderr);
    assert_eq!(out.stdout, "0000000");
    let expected = [
        ctype_deprecation("ctype_digit", "bool"),
        ctype_deprecation("ctype_print", "float"),
        ctype_deprecation("ctype_space", "null"),
        ctype_deprecation("ctype_alpha", "array"),
        ctype_deprecation("ctype_alnum", "Token"),
        ctype_deprecation("ctype_graph", "resource"),
        ctype_deprecation("ctype_lower", "Closure"),
    ];
    assert_eq!(normalize_diagnostic_text(&out.stderr), expected.join("\n") + "\n");
}

/// Verifies boxed `mixed` and nullable-int arguments dispatch on their runtime type.
///
/// Strings in the boxes are scanned without a deprecation, ints follow the byte rules, and
/// the other payloads deprecate and fail. Before the `mixed` parameter, a boxed `50` was
/// converted to `"50"` and accepted by `ctype_digit()` as digits instead of byte `'2'`.
#[test]
fn test_ctype_boxed_and_nullable_arguments_follow_runtime_type() {
    let out = compile_and_run_capture(
        r#"<?php
class Token {}
$values = ["12", 50, -200, 1000, 2.5, null, true, ["x"], new Token(), "", "\xff"];
foreach ($values as $value) {
    echo ctype_digit($value) ? "1" : "0", ctype_graph($value) ? "1" : "0", " ";
}
function digits(?int $value): bool { return ctype_digit($value); }
echo digits(55) ? "1" : "0", digits(null) ? "1" : "0", digits(4096) ? "1" : "0";
"#,
    );
    assert!(out.success, "stderr: {}", out.stderr);
    assert_eq!(out.stdout, "11 11 01 11 00 00 00 00 00 00 00 101");
    let mut expected = Vec::new();
    for type_name in ["int", "int", "int", "float", "null", "bool", "array", "Token"] {
        expected.push(ctype_deprecation("ctype_digit", type_name));
        expected.push(ctype_deprecation("ctype_graph", type_name));
    }
    for type_name in ["int", "null", "int"] {
        expected.push(ctype_deprecation("ctype_digit", type_name));
    }
    assert_eq!(normalize_diagnostic_text(&out.stderr), expected.join("\n") + "\n");
}

/// Verifies `ctype_*` builtins work as runtime-selected callables.
///
/// A name held in a loop variable used to stop with `Call to undefined function <dynamic>()`
/// because the builtins had no runtime wrapper. The fixture also covers `array_map()`,
/// `call_user_func()` with a variable name, named `call_user_func_array()` arguments, and a
/// first-class callable.
#[test]
fn test_ctype_runtime_selected_callables() {
    let out = compile_and_run_capture(
        r#"<?php
foreach (["ctype_alpha", "ctype_digit", "ctype_xdigit"] as $name) {
    echo $name("ab") ? "1" : "0", $name("12") ? "1" : "0";
}
echo ":";
echo implode("", array_map(fn($ok) => $ok ? "1" : "0", array_map("ctype_upper", ["A", "b", "AB"])));
echo ":";
$name = "ctype_space";
echo call_user_func($name, " \n") ? "1" : "0";
echo call_user_func_array("ctype_lower", ["text" => "ab"]) ? "1" : "0";
$punct = ctype_punct(...);
echo $punct("!?") ? "1" : "0", $punct("a!") ? "1" : "0";
"#,
    );
    assert!(out.success, "stderr: {}", out.stderr);
    assert_eq!(out.stdout, "100111:101:1110");
    assert_eq!(out.stderr, "");
}

/// Verifies the deprecation goes through PHP's error-handler and reporting machinery.
///
/// A throwing handler receives `E_DEPRECATED` (8192) and unwinds out of the call before a
/// result is produced, string arguments never reach the handler, and masking
/// `E_DEPRECATED` silences the diagnostic without changing the result.
#[test]
fn test_ctype_deprecation_uses_error_handlers_and_reporting() {
    let out = compile_and_run_capture(
        r#"<?php
set_error_handler(function (int $level, string $message): bool {
    throw new RuntimeException("[$level] $message");
});
$values = [65, "A"];
try {
    echo ctype_upper($values[0]) ? "pass" : "fail";
} catch (RuntimeException $error) {
    echo $error->getMessage(), "\n";
}
echo ctype_upper($values[1]) ? "string:pass" : "string:fail", "\n";
restore_error_handler();
error_reporting(E_ALL & ~E_DEPRECATED);
echo ctype_upper(65) ? "quiet:pass" : "quiet:fail";
"#,
    );
    assert!(out.success, "stderr: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "[8192] ctype_upper(): Argument of type int will be interpreted as string in the future\n\
         string:pass\nquiet:pass"
    );
    assert_eq!(out.stderr, "");
}
