//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of strings formatting, including sprintf string, sprintf integer, and sprintf multiple.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

/// Tests sprintf with %s string replacement.
#[test]
fn test_sprintf_string() {
    let out = compile_and_run(r#"<?php echo sprintf("Hello %s", "World");"#);
    assert_eq!(out, "Hello World");
}

/// Tests sprintf with %d integer formatting.
#[test]
fn test_sprintf_int() {
    let out = compile_and_run(r#"<?php echo sprintf("Value: %d", 42);"#);
    assert_eq!(out, "Value: 42");
}

/// Tests sprintf with multiple format specifiers (%s and %d) in one format string.
#[test]
fn test_sprintf_multiple() {
    let out = compile_and_run(r#"<?php echo sprintf("%s is %d", "age", 30);"#);
    assert_eq!(out, "age is 30");
}

/// Tests sprintf with %% escape sequence producing a literal percent sign.
#[test]
fn test_sprintf_percent() {
    let out = compile_and_run(r#"<?php echo sprintf("100%%");"#);
    assert_eq!(out, "100%");
}

/// Tests sprintf with %.2f precision specifier limiting float to two decimal places.
#[test]
fn test_sprintf_precision_float() {
    let out = compile_and_run(r#"<?php echo sprintf("%.2f", 3.14159);"#);
    assert_eq!(out, "3.14");
}

/// Tests sprintf with %10s width specifier right-padding a string to 10 characters.
#[test]
fn test_sprintf_width_string() {
    let out = compile_and_run(r#"<?php echo sprintf("%10s", "hi");"#);
    assert_eq!(out, "        hi");
}

/// Tests sprintf with %-10s left-alignment specifier and pipe delimiter to confirm trailing spaces.
#[test]
fn test_sprintf_left_align_string() {
    let out = compile_and_run(r#"<?php echo sprintf("%-10s|", "hi");"#);
    assert_eq!(out, "hi        |");
}

/// Tests sprintf with %+d force-sign specifier forcing a plus sign on positive integers.
#[test]
fn test_sprintf_plus_sign() {
    let out = compile_and_run(r#"<?php echo sprintf("%+d", 42);"#);
    assert_eq!(out, "+42");
}

/// Tests sprintf with %.5f precision specifier preserving trailing zeros on 1.0.
#[test]
fn test_sprintf_precision_float_trailing_zeros() {
    let out = compile_and_run(r#"<?php echo sprintf("%.5f", 1.0);"#);
    assert_eq!(out, "1.00000");
}

/// Tests sprintf with bare %f default precision (6 decimal places).
#[test]
fn test_sprintf_float_default() {
    let out = compile_and_run(r#"<?php echo sprintf("%f", 3.14);"#);
    assert_eq!(out, "3.140000");
}

/// Tests printf (output to stdout) with %s string replacement.
#[test]
fn test_printf() {
    let out = compile_and_run(r#"<?php printf("Hello %s", "World");"#);
    assert_eq!(out, "Hello World");
}

/// Verifies an integer argument under a `%s` specifier is coerced to its string form,
/// matching PHP's `sprintf("%s", 42)` → "42" rather than producing an empty string.
#[test]
fn test_sprintf_int_under_string_specifier() {
    let out = compile_and_run(r#"<?php echo sprintf("%s", 42);"#);
    assert_eq!(out, "42");
}

/// Verifies a string argument under a `%d` specifier is parsed as a leading-numeric int,
/// matching PHP's `sprintf("%d", "42abc")` → "42" rather than printing a pointer value.
#[test]
fn test_sprintf_string_under_int_specifier() {
    let out = compile_and_run(r#"<?php echo sprintf("%d", "42abc");"#);
    assert_eq!(out, "42");
}

/// Verifies an integer argument under a float specifier is widened to a double,
/// matching PHP's `sprintf("%.1f", 3)` → "3.0".
#[test]
fn test_sprintf_int_under_float_specifier() {
    let out = compile_and_run(r#"<?php echo sprintf("%.1f", 3);"#);
    assert_eq!(out, "3.0");
}

/// Verifies a float argument under a `%d` specifier is truncated toward zero,
/// matching PHP's `sprintf("%d", 3.9)` → "3".
#[test]
fn test_sprintf_float_under_int_specifier() {
    let out = compile_and_run(r#"<?php echo sprintf("%d", 3.9);"#);
    assert_eq!(out, "3");
}

/// Verifies `Mixed` arguments (heterogeneous associative-array values) are coerced to the
/// type each specifier consumes: an int-bearing value under `%d` and a string-bearing value
/// under `%s` both format correctly instead of pushing a zero/garbage payload.
#[test]
fn test_sprintf_mixed_arguments() {
    let out = compile_and_run(
        r#"<?php
$a = ["n" => 42, "s" => "hi"];
echo sprintf("%d", $a["n"]);
echo ",";
echo sprintf("%s", $a["s"]);
"#,
    );
    assert_eq!(out, "42,hi");
}

/// Verifies cross-type `Mixed` formatting matches PHP: a numeric `Mixed` under `%s` stringifies
/// and a non-numeric `Mixed` string under `%d` casts to 0, like `sprintf("%s|%d", 42, "hi")`.
#[test]
fn test_sprintf_mixed_cross_type() {
    let out = compile_and_run(
        r#"<?php
$a = ["n" => 42, "s" => "hi"];
echo sprintf("%s|%d", $a["n"], $a["s"]);
"#,
    );
    assert_eq!(out, "42|0");
}

/// Verifies printf applies the same specifier-driven coercion as sprintf for cross-type
/// arguments (int under `%05d`, plain string), writing the formatted bytes to stdout.
#[test]
fn test_printf_cross_type_arguments() {
    let out = compile_and_run(r#"<?php printf("[%05d] %s", "7abc", 99);"#);
    assert_eq!(out, "[00007] 99");
}

// --- String interpolation ---

/// Tests sscanf with %d parsing an integer from a formatted string.
#[test]
fn test_sscanf_int() {
    let out = compile_and_run(
        r#"<?php
$result = sscanf("Age: 25", "Age: %d");
echo $result[0];
"#,
    );
    assert_eq!(out, "25");
}

/// Tests sscanf with %s parsing a word from a formatted string.
#[test]
fn test_sscanf_string() {
    let out = compile_and_run(
        r#"<?php
$result = sscanf("Name: Alice", "Name: %s");
echo $result[0];
"#,
    );
    assert_eq!(out, "Alice");
}

/// Tests sscanf with multiple format specifiers (%s and %d) parsing two values into an array.
#[test]
fn test_sscanf_multiple() {
    let out = compile_and_run(
        r#"<?php
$result = sscanf("John 30", "%s %d");
echo $result[0] . " " . $result[1];
"#,
    );
    assert_eq!(out, "John 30");
}

/// sscanf %f captures a float slice. Like %d, sscanf returns the matched
/// substring (Array(Str)), so the assertion compares the captured text.
#[test]
fn test_sscanf_float() {
    let out = compile_and_run(
        r#"<?php
$r = sscanf("Pi: 3.14", "Pi: %f");
echo $r[0];
"#,
    );
    assert_eq!(out, "3.14");
}

/// %f accepts a leading sign and a scientific exponent.
#[test]
fn test_sscanf_float_negative_and_exponent() {
    let out = compile_and_run(
        r#"<?php
$r = sscanf("-2.5e3", "%f");
echo $r[0];
"#,
    );
    assert_eq!(out, "-2.5e3");
}

/// %f composes with %s and %d in one format, each capturing its slice.
#[test]
fn test_sscanf_float_mixed_with_string_and_int() {
    let out = compile_and_run(
        r#"<?php
$r = sscanf("alice 1.5 30", "%s %f %d");
echo $r[0] . "|" . $r[1] . "|" . $r[2];
"#,
    );
    assert_eq!(out, "alice|1.5|30");
}

/// The string-search builtins must coerce a Mixed/Union haystack (e.g. a
/// `string|false` value, as returned by stream_socket_get_name) to a real
/// string before searching. The bug: they passed the boxed Mixed cell straight
/// to the runtime helper (no coerce_to_string), which found no match on x86_64.
/// The fix routes the operands through emit_string_arg. `$h` here is Union via
/// the ternary, exercising the coercion on both arches.
#[test]
fn test_string_search_with_mixed_haystack() {
    let out = compile_and_run(
        r#"<?php
$h = (strlen("x") > 0) ? "hello world" : false;
echo "p=" . strpos($h, "world");
echo "|c=" . (str_contains($h, "world") ? "Y" : "N");
echo "|s=" . (str_starts_with($h, "hello") ? "Y" : "N");
echo "|e=" . (str_ends_with($h, "world") ? "Y" : "N");
echo "|ss=[" . strstr($h, "wor") . "]";
echo "|r=" . strrpos($h, "o");
"#,
    );
    assert_eq!(out, "p=6|c=Y|s=Y|e=Y|ss=[world]|r=7");
}

/// Verifies compiled PHP output for vsprintf vprintf vfprintf.
#[test]
fn test_vsprintf_vprintf_vfprintf() {
    // OOS Phase G: vsprintf/vprintf/vfprintf format with the arguments supplied
    // as an array (the __rt_vsprintf bridge pushes one tagged record per element
    // and tail-calls __rt_sprintf). Covers a heterogeneous Mixed array, a
    // homogeneous int array, and a string array, plus vprintf's stdout write +
    // length return and vfprintf writing to a php://temp stream.
    let out = compile_and_run(
        r#"<?php
echo vsprintf("%s is %d (%.1f)", ["age", 42, 3.5]);
echo "|" . vsprintf("%d-%d-%d", [1, 2, 3]);
echo "|" . vsprintf("%s/%s", ["a", "b"]);
$n = vprintf("|[%s=%d]", ["x", 7]);
echo "|n=" . $n;
$f = fopen("php://temp", "w+");
$m = vfprintf($f, "%d:%s", [9, "z"]);
rewind($f);
echo "|f=" . stream_get_contents($f) . "|m=" . $m;
fclose($f);
"#,
    );
    assert_eq!(out, "age is 42 (3.5)|1-2-3|a/b|[x=7]|n=6|f=9:z|m=3");
}

/// `printf()` returns the number of bytes written, matching PHP. Regression for
/// an x86_64-specific bug where the byte count was parked in `rcx` across the
/// `write` syscall — which the `syscall` instruction clobbers — so the return
/// value was garbage on x86_64 while correct on ARM64.
#[test]
fn test_printf_returns_byte_count() {
    let out = compile_and_run(
        r#"<?php
$n = printf("[%s=%d]", "x", 42);
echo "|n=" . $n;
"#,
    );
    assert_eq!(out, "[x=42]|n=6");
}
