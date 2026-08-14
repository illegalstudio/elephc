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

// --- printf-family memory safety and PHP conversion parity ---

/// A field width wider than the per-conversion scratch buffer must produce padding bytes
/// only. Regression for an out-of-bounds stack read: the helper used to copy `snprintf`'s
/// "would have written" return value out of a 128-byte stack buffer, so `sprintf("[%300s]",
/// "x")` leaked ~170 bytes of live stack (including pointers) into the result string.
#[test]
fn test_sprintf_wide_width_emits_only_padding() {
    let out = compile_and_run(
        r#"<?php
$r = sprintf("[%300s]", "x");
echo strlen($r), "|", bin2hex($r) === "5b" . str_repeat("20", 299) . "78" . "5d" ? "clean" : "leak";
"#,
    );
    assert_eq!(out, "302|clean");
}

/// A `%s` operand longer than the old 128-byte null-termination buffer must survive intact.
/// The helper used to clamp string operands to 127 bytes.
#[test]
fn test_sprintf_long_string_argument_is_not_truncated() {
    let out = compile_and_run(r#"<?php echo strlen(sprintf("%s", str_repeat("a", 200)));"#);
    assert_eq!(out, "200");
}

/// A width past `INT_MAX` must be a controlled error, not silent stack corruption. The
/// specifier used to be copied byte-for-byte into a 32-byte stack buffer, so a 40-digit
/// width overwrote adjacent frame slots and a 300-digit width overwrote the saved return
/// address (these binaries carry no stack canary).
#[test]
fn test_sprintf_overlong_width_reports_runtime_error() {
    let err = compile_and_run_expect_failure(
        r#"<?php echo sprintf("%" . str_repeat("9", 300) . "s", "x");"#,
    );
    assert!(
        err.contains("Width must be between 0 and 2147483647"),
        "{err}"
    );
}

/// A width that fits in `INT_MAX` but not in the 64 KiB result arena must be a controlled
/// error too; `sprintf("%2000000000s", "x")` used to segfault.
#[test]
fn test_sprintf_result_larger_than_buffer_reports_runtime_error() {
    let err = compile_and_run_expect_failure(r#"<?php echo sprintf("%2000000000s", "x");"#);
    assert!(
        err.contains("formatted result exceeds the 65536-byte string buffer"),
        "{err}"
    );
}

/// A format string that consumes more arguments than were supplied must stop instead of
/// reading past the pushed argument records on the caller's stack.
#[test]
fn test_sprintf_missing_argument_reports_runtime_error() {
    let err = compile_and_run_expect_failure(r#"<?php $f = "%s%s"; echo sprintf($f, "a");"#);
    assert!(err.contains("too few arguments"), "{err}");
}

/// Conversion characters PHP does not define must never be forwarded to libc `snprintf`
/// (which would expose `%n`, an arbitrary-write primitive). PHP raises a `ValueError`.
#[test]
fn test_sprintf_unknown_specifier_reports_runtime_error() {
    let err = compile_and_run_expect_failure(r#"<?php $f = "%n"; echo sprintf($f, 5);"#);
    assert!(err.contains("Unknown format specifier"), "{err}");
}

/// `%b` renders binary, matching PHP's `sprintf("%b", 5)` → `101`. libc has no portable
/// `%b`, so the old code emitted the literal specifier text instead.
#[test]
fn test_sprintf_binary_specifier() {
    let out = compile_and_run(
        r#"<?php echo sprintf("%b", 5), "|", sprintf("%b", 0), "|", sprintf("%'x5b", 5);"#,
    );
    assert_eq!(out, "101|0|xx101");
}

/// `%F` is PHP's locale-independent fixed-point conversion and must format like `%f`.
#[test]
fn test_sprintf_uppercase_fixed_point() {
    let out = compile_and_run(r#"<?php echo sprintf("%F", 3.5);"#);
    assert_eq!(out, "3.500000");
}

/// `%E` must render the uppercase scientific form. It used to build the invalid libc
/// format `%llE`, printing an uninitialized register as a denormal.
#[test]
fn test_sprintf_uppercase_scientific() {
    let out = compile_and_run(r#"<?php echo sprintf("%E", 12345.678);"#);
    assert_eq!(out, "1.234568E+4");
}

/// PHP does not zero-pad the `%e`/`%E` exponent to two digits the way C does.
#[test]
fn test_sprintf_exponent_is_not_zero_padded() {
    let out = compile_and_run(
        r#"<?php echo sprintf("%e", 12345.678), "|", sprintf("%e", 1.0), "|", sprintf("%e", 1e-100), "|", sprintf("%.3e", 12345.678);"#,
    );
    assert_eq!(out, "1.234568e+4|1.000000e+0|1.000000e-100|1.235e+4");
}

/// PHP's `%f`/`%e` renderer drops the sign of negative zero, while `%g` keeps it.
#[test]
fn test_sprintf_negative_zero() {
    let out = compile_and_run(
        r#"<?php echo sprintf("%f", -0.0), "|", sprintf("%e", -0.0), "|", sprintf("%g", -0.0);"#,
    );
    assert_eq!(out, "0.000000|0.000000e+0|-0");
}

/// PHP's `N$` explicit argument numbers select an operand without advancing the sequential
/// cursor, so one argument can be formatted twice.
#[test]
fn test_sprintf_positional_arguments() {
    let out = compile_and_run(
        r#"<?php echo sprintf('%1$s-%1$s', "x"), "|", sprintf('%2$s %1$s', "a", "b");"#,
    );
    assert_eq!(out, "x-x|b a");
}

/// PHP's `'X` flag sets a custom pad character; `X` must not be mistaken for the width or
/// the conversion character.
#[test]
fn test_sprintf_custom_pad_character() {
    let out = compile_and_run(
        r#"<?php echo sprintf("%'.4f", 1.5), "|", sprintf("%'x10d", -42), "|", sprintf("%'*8.3f", -1.5), "|", sprintf("%-'x10s", "ab");"#,
    );
    assert_eq!(out, "1.500000|xxxxxxx-42|**-1.500|abxxxxxxxx");
}

/// Zero padding is inserted after a leading sign, matching PHP's `sprintf("%05d", -42)`.
#[test]
fn test_sprintf_zero_padding_follows_the_sign() {
    let out = compile_and_run(
        r#"<?php echo sprintf("%05d", -42), "|", sprintf("%010.2f", -1.5), "|", sprintf("%08.3f", -1.5);"#,
    );
    assert_eq!(out, "-0042|-000001.50|-001.500");
}

/// PHP appends `%c` without applying width or padding, and clamps float precision to 53
/// digits instead of rendering an unbounded fraction.
#[test]
fn test_sprintf_char_and_precision_limits() {
    let out = compile_and_run(
        r#"<?php echo sprintf("%5c", 65), "|", strlen(sprintf("%.100f", 1.5)), "|", sprintf("%.5d", 42);"#,
    );
    assert_eq!(out, "A|55|42");
}

/// When the conversion character and the packed operand disagree the runtime must coerce the
/// operand, never print it as a raw pointer. This happens with a format string built at
/// runtime, with `v*printf()` over a heterogeneous array, and with `%1$s`/`%1$d` naming the
/// same argument under two conversions.
#[test]
fn test_sprintf_operand_type_mismatch_is_coerced_not_leaked() {
    let out = compile_and_run(
        r#"<?php
$f = "%d";
echo sprintf($f, "42");
echo "|" . vsprintf("%d", ["42"]);
echo "|" . vsprintf("%f", ["3.5"]);
$g = "%s";
echo "|" . sprintf($g, 42);
echo "|" . sprintf('%1$s %1$d %1$s', "7");
echo "|" . sprintf('%1$s %1$f', "3.5");
"#,
    );
    assert_eq!(out, "42|42|3.500000|42|7 7 7|3.5 3.500000");
}

/// Verifies `sprintf()` keeps a `mixed` argument when the format string is not a literal.
///
/// The test above already covered a runtime format string — but with a STATICALLY TYPED value
/// (`$f = "%d"; sprintf($f, "42")`), which is why it passed while this was broken. The defect
/// needed BOTH halves: no literal format, so no conversion category is known at compile time,
/// AND a `mixed` operand, which then fell into the argument packer's catch-all arm and was
/// pushed as a zero payload tagged as an integer.
///
/// A heterogeneous array produces exactly that pair, and it is the shape real code writes: a
/// table of `[format, value]` rows. Every row below printed `0` before.
///
/// `echo` rendered the same values correctly throughout, which is what made this read as a
/// formatting bug rather than an argument-marshalling one.
#[test]
fn test_sprintf_keeps_a_mixed_argument_under_a_runtime_format() {
    let out = compile_and_run(
        r#"<?php
foreach ([["%5d", 42], ["%x", 255], ["%s", "hi"], ["%.2f", 1.5], ["%d", true]] as $row) {
    echo "[", sprintf($row[0], $row[1]), "]";
}
"#,
    );
    assert_eq!(out, "[   42][ff][hi][1.50][1]");
}

/// Verifies a `null` argument formats as PHP's empty string, not as the internal null sentinel.
///
/// `vsprintf("%s", [null])` answered `9223372036854775806` — `0x7FFF_FFFF_FFFF_FFFE`, the raw
/// null sentinel, printed straight into the output. The record ladder sent a boxed null down its
/// "anything else, treat as an integer" arm, whose payload is that sentinel.
///
/// PHP renders `null` as `""` under `%s` and `0` under `%d`. Packing it as a ZERO-LENGTH STRING
/// gives both, because the formatter already guards a null string pointer on every conversion
/// path. Both spellings are asserted: they now share one ladder, and this is the assertion that
/// would catch them drifting apart again.
#[test]
fn test_null_formats_as_empty_string_not_the_internal_sentinel() {
    let out = compile_and_run(
        r#"<?php
echo "[", sprintf("%s", null), "]";
echo "[", vsprintf("%s", [null]), "]";
echo "[", sprintf("%d", null), "]";
echo "[", vsprintf("%d", [null]), "]";
$f = "%s";
echo "[", sprintf($f, null), "]";
"#,
    );
    assert_eq!(out, "[][][0][0][]");
}
