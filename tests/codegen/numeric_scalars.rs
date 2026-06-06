//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of numeric scalars, including echo float, echo float integer value, and echo negative float.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use crate::support::*;

// --- Float literals ---

/// Verifies that a plain float literal (3.14) is echoed correctly.
#[test]
fn test_echo_float() {
    let out = compile_and_run("<?php echo 3.14;");
    assert_eq!(out, "3.14");
}

/// Verifies that a float with integer value (4.0) echoes without the decimal part, matching PHP's output format.
#[test]
fn test_echo_float_integer_value() {
    let out = compile_and_run("<?php echo 4.0;");
    assert_eq!(out, "4");
}

/// Verifies that a negative float literal (-3.14) is echoed correctly with the minus sign.
#[test]
fn test_echo_negative_float() {
    let out = compile_and_run("<?php echo -3.14;");
    assert_eq!(out, "-3.14");
}

/// Verifies that a dot-prefix float literal (.5) is accepted and outputs 0.5.
#[test]
fn test_echo_dot_prefix_float() {
    let out = compile_and_run("<?php echo .5;");
    assert_eq!(out, "0.5");
}

// --- Float arithmetic ---

/// Verifies that two float literals add correctly, producing 3.8.
#[test]
fn test_float_addition() {
    let out = compile_and_run("<?php echo 1.5 + 2.3;");
    assert_eq!(out, "3.8");
}

/// Verifies that float subtraction (5.5 - 2.2) produces the correct result.
#[test]
fn test_float_subtraction() {
    let out = compile_and_run("<?php echo 5.5 - 2.2;");
    assert_eq!(out, "3.3");
}

/// Verifies that float multiplication (3.0 * 2.5) produces 7.5.
#[test]
fn test_float_multiplication() {
    let out = compile_and_run("<?php echo 3.0 * 2.5;");
    assert_eq!(out, "7.5");
}

/// Verifies that float division (7.5 / 2.5) produces 3.
#[test]
fn test_float_division() {
    let out = compile_and_run("<?php echo 7.5 / 2.5;");
    assert_eq!(out, "3");
}

// --- Mixed int+float ---

/// Verifies int + float (10 + 0.5) produces 10.5.
#[test]
fn test_int_plus_float() {
    let out = compile_and_run("<?php echo 10 + 0.5;");
    assert_eq!(out, "10.5");
}

/// Verifies float + int (0.5 + 10) produces 10.5.
#[test]
fn test_float_plus_int() {
    let out = compile_and_run("<?php echo 0.5 + 10;");
    assert_eq!(out, "10.5");
}

/// Verifies int * float (3 * 1.5) produces 4.5.
#[test]
fn test_int_times_float() {
    let out = compile_and_run("<?php echo 3 * 1.5;");
    assert_eq!(out, "4.5");
}

// --- Float comparison ---

/// Verifies float greater-than comparison (3.14 > 2.0) echoes 1.
#[test]
fn test_float_greater_than() {
    let out = compile_and_run("<?php echo 3.14 > 2.0;");
    assert_eq!(out, "1");
}

/// Verifies float less-than comparison (1.5 < 2.5) echoes 1.
#[test]
fn test_float_less_than() {
    let out = compile_and_run("<?php echo 1.5 < 2.5;");
    assert_eq!(out, "1");
}

/// Verifies float equality comparison (3.14 == 3.14) echoes 1.
#[test]
fn test_float_equal() {
    let out = compile_and_run("<?php echo 3.14 == 3.14;");
    assert_eq!(out, "1");
}

/// Verifies float not-equal comparison (3.14 != 2.0) echoes 1.
#[test]
fn test_float_not_equal() {
    let out = compile_and_run("<?php echo 3.14 != 2.0;");
    assert_eq!(out, "1");
}

// --- Float concatenation ---

/// Verifies string concat with float on the right ("pi=" . 3.14) produces "pi=3.14".
#[test]
fn test_float_concat() {
    let out = compile_and_run("<?php echo \"pi=\" . 3.14;");
    assert_eq!(out, "pi=3.14");
}

/// Verifies string concat with float on the left (3.14 . " is pi") produces "3.14 is pi".
#[test]
fn test_float_concat_reverse() {
    let out = compile_and_run("<?php echo 3.14 . \" is pi\";");
    assert_eq!(out, "3.14 is pi");
}

// --- Math functions ---

/// Verifies floor() rounds a positive float (3.7) down to 3.
#[test]
fn test_floor() {
    let out = compile_and_run("<?php echo floor(3.7);");
    assert_eq!(out, "3");
}

/// Verifies single-arg math builtins coerce a boxed Mixed argument to a real float
/// (H1): a heterogeneous array makes its elements Mixed, and the math builtin must
/// unbox the cell rather than treat the cell pointer as an integer/float.
#[test]
fn test_math_builtins_coerce_mixed_arg() {
    // $a[0] is a Mixed cell holding 2.7 (the array is heterogeneous → element type Mixed).
    assert_eq!(compile_and_run(r#"<?php $a = [2.7, "s"]; echo floor($a[0]);"#), "2");
    assert_eq!(compile_and_run(r#"<?php $a = [2.7, "s"]; echo ceil($a[0]);"#), "3");
    assert_eq!(
        compile_and_run(r#"<?php $a = [6.25, "s"]; echo (int) sqrt($a[0]);"#),
        "2"
    );
    // sin(0.0) == 0.0; a Mixed 0.0 must unbox to the float, not the pointer.
    assert_eq!(
        compile_and_run(r#"<?php $a = [0.0, "s"]; echo (sin($a[0]) == 0.0) ? "ok" : "bad";"#),
        "ok"
    );
}

/// Verifies ceil() rounds a positive float (3.2) up to 4.
#[test]
fn test_ceil() {
    let out = compile_and_run("<?php echo ceil(3.2);");
    assert_eq!(out, "4");
}

/// Verifies round() rounds a float at .5 threshold (3.5) up to 4 (banker's rounding disabled).
#[test]
fn test_round() {
    let out = compile_and_run("<?php echo round(3.5);");
    assert_eq!(out, "4");
}

/// Verifies round() rounds a float below .5 threshold (3.4) down to 3.
#[test]
fn test_round_down() {
    let out = compile_and_run("<?php echo round(3.4);");
    assert_eq!(out, "3");
}

/// Verifies sqrt() of a perfect square (16.0) produces 4.
#[test]
fn test_sqrt() {
    let out = compile_and_run("<?php echo sqrt(16.0);");
    assert_eq!(out, "4");
}

/// Verifies sqrt() of a non-perfect square (2.0) produces an accurate decimal result.
#[test]
fn test_sqrt_non_perfect() {
    let out = compile_and_run("<?php echo sqrt(2.0);");
    assert_eq!(out, "1.4142135623731");
}

/// Verifies abs() on a negative float (-3.14) returns 3.14.
#[test]
fn test_abs_float() {
    let out = compile_and_run("<?php echo abs(-3.14);");
    assert_eq!(out, "3.14");
}

/// Verifies abs() on a negative integer (-42) returns 42.
#[test]
fn test_abs_int() {
    let out = compile_and_run("<?php echo abs(-42);");
    assert_eq!(out, "42");
}

/// Verifies pow() with float base and exponent (2.0, 10.0) produces 1024.
#[test]
fn test_pow() {
    let out = compile_and_run("<?php echo pow(2.0, 10.0);");
    assert_eq!(out, "1024");
}

/// Verifies min() with two integers (3, 7) returns the smaller (3).
#[test]
fn test_min_int() {
    let out = compile_and_run("<?php echo min(3, 7);");
    assert_eq!(out, "3");
}

/// Verifies max() with two integers (3, 7) returns the larger (7).
#[test]
fn test_max_int() {
    let out = compile_and_run("<?php echo max(3, 7);");
    assert_eq!(out, "7");
}

/// Verifies min() with two floats (1.5, 2.5) returns the smaller (1.5).
#[test]
fn test_min_float() {
    let out = compile_and_run("<?php echo min(1.5, 2.5);");
    assert_eq!(out, "1.5");
}

/// Verifies max() with two floats (1.5, 2.5) returns the larger (2.5).
#[test]
fn test_max_float() {
    let out = compile_and_run("<?php echo max(1.5, 2.5);");
    assert_eq!(out, "2.5");
}

/// Verifies intdiv() with integers (7, 2) performs integer division returning 3.
#[test]
fn test_intdiv() {
    let out = compile_and_run("<?php echo intdiv(7, 2);");
    assert_eq!(out, "3");
}

// --- Type checking builtins ---

/// Verifies floatval() converts an integer (42) to a float, echoing "42".
#[test]
fn test_floatval() {
    let out = compile_and_run("<?php echo floatval(42);");
    assert_eq!(out, "42");
}

/// Verifies is_float() returns 1 for a float value (3.14).
#[test]
fn test_is_float_true() {
    let out = compile_and_run("<?php echo is_float(3.14);");
    assert_eq!(out, "1");
}

/// Verifies is_float() returns empty string for an integer (42).
#[test]
fn test_is_float_false() {
    let out = compile_and_run("<?php echo is_float(42);");
    assert_eq!(out, "");
}

/// Verifies is_int() returns 1 for an integer value (42).
#[test]
fn test_is_int_true() {
    let out = compile_and_run("<?php echo is_int(42);");
    assert_eq!(out, "1");
}

/// Verifies is_int() returns empty string for a float (3.14).
#[test]
fn test_is_int_false() {
    let out = compile_and_run("<?php echo is_int(3.14);");
    assert_eq!(out, "");
}

// --- Float variable ---

/// Verifies that a float can be assigned to a variable and echoed.
#[test]
fn test_float_variable() {
    let out = compile_and_run("<?php $x = 3.14; echo $x;");
    assert_eq!(out, "3.14");
}

/// Verifies float variable arithmetic ($a = 1.5; $b = 2.5; $a + $b) returns 4 (integer-formatted).
#[test]
fn test_float_variable_arithmetic() {
    let out = compile_and_run("<?php $a = 1.5; $b = 2.5; echo $a + $b;");
    assert_eq!(out, "4");
}

/// Verifies a float variable used in a comparison condition (3.14 > 3.0) selects the correct branch.
#[test]
fn test_float_in_condition() {
    let out =
        compile_and_run("<?php $x = 3.14; if ($x > 3.0) { echo \"yes\"; } else { echo \"no\"; }");
    assert_eq!(out, "yes");
}

// --- Large integer literal promotion ---

/// Verifies that the maximum 64-bit integer literal (9223372036854775807 / 0x7FFFFFFFFFFFFFFF) stays integer type.
#[test]
fn test_max_integer_literal_stays_integer() {
    let out = compile_and_run(
        "<?php echo gettype(9223372036854775807) . \"|\" . gettype(0x7FFFFFFFFFFFFFFF);",
    );
    assert_eq!(out, "integer|integer");
}

/// Verifies that one past the max 64-bit integer literal (9223372036854775808) promotes to double.
#[test]
fn test_large_decimal_integer_literal_promotes_to_float() {
    let out = compile_and_run("<?php echo gettype(9223372036854775808);");
    assert_eq!(out, "double");
}

/// Verifies that large hex, binary, and octal literals that overflow 64-bit promote to double.
#[test]
fn test_large_radix_integer_literals_promote_to_float() {
    let out = compile_and_run(
        "<?php
        echo gettype(0xFFFFFFFFFFFFFFFF) . \"|\";
        echo gettype(0b1111111111111111111111111111111111111111111111111111111111111111) . \"|\";
        echo gettype(01777777777777777777777);
        ",
    );
    assert_eq!(out, "double|double|double");
}

// --- Octal integer literals ---

/// Verifies that PHP 8.0+ octal literal syntax (0o777) echoes 511.
#[test]
fn test_octal_literal_echo() {
    let out = compile_and_run("<?php echo 0o777;");
    assert_eq!(out, "511");
}

/// Verifies that the legacy octal literal syntax (0777) echoes 511.
#[test]
fn test_legacy_octal_literal_echo() {
    let out = compile_and_run("<?php echo 0777;");
    assert_eq!(out, "511");
}

/// Verifies that the PHP 7.1+ octal separator syntax (0_777) echoes 511.
#[test]
fn test_legacy_octal_separator_echo() {
    let out = compile_and_run("<?php echo 0_777;");
    assert_eq!(out, "511");
}

/// Verifies that 08.5 (leading zero before a decimal float) is treated as decimal 8.5, not octal.
#[test]
fn test_leading_zero_float_is_decimal() {
    let out = compile_and_run("<?php echo 08.5;");
    assert_eq!(out, "8.5");
}

/// Verifies octal literals work as default parameter values in user functions.
#[test]
fn test_octal_literal_default_param() {
    let out = compile_and_run(
        "<?php
        function default_mode(int $mode = 0o777): int {
            return $mode;
        }
        echo default_mode();
        ",
    );
    assert_eq!(out, "511");
}

// --- Binary integer literals ---

/// Verifies that binary literal syntax (0b1010) echoes 10.
#[test]
fn test_binary_literal_echo() {
    let out = compile_and_run("<?php echo 0b1010;");
    assert_eq!(out, "10");
}

/// Verifies that binary literals support bitwise operations (0b1100 & 0b1010) produces 8.
#[test]
fn test_binary_literal_arith() {
    let out = compile_and_run("<?php echo 0b1100 & 0b1010;");
    assert_eq!(out, "8");
}

/// Verifies that uppercase binary prefix (0B11111111) is accepted and echoes 255.
#[test]
fn test_binary_literal_uppercase() {
    let out = compile_and_run("<?php echo 0B11111111;");
    assert_eq!(out, "255");
}

// --- Numeric separators (PHP 7.4+) ---

/// Verifies that decimal numeric separators (1_000_000) are accepted and echo 1000000.
#[test]
fn test_decimal_separator_echo() {
    let out = compile_and_run("<?php echo 1_000_000;");
    assert_eq!(out, "1000000");
}

/// Verifies that hex numeric separators (0xFF_FF) are accepted and echo 65535.
#[test]
fn test_hex_separator_echo() {
    let out = compile_and_run("<?php echo 0xFF_FF;");
    assert_eq!(out, "65535");
}

/// Verifies that octal numeric separators (0o7_7_7) are accepted and echo 511.
#[test]
fn test_octal_separator_echo() {
    let out = compile_and_run("<?php echo 0o7_7_7;");
    assert_eq!(out, "511");
}

/// Verifies that binary numeric separators (0b1010_1010) are accepted and echo 170.
#[test]
fn test_binary_separator_echo() {
    let out = compile_and_run("<?php echo 0b1010_1010;");
    assert_eq!(out, "170");
}

/// Verifies that decimal numeric separators work in floats (1_000.5) and echo 1000.5.
#[test]
fn test_float_separator_echo() {
    let out = compile_and_run("<?php echo 1_000.5;");
    assert_eq!(out, "1000.5");
}

/// Verifies that numeric separators can appear in the exponent part of float literals (1e1_0) and echo 10000000000.
#[test]
fn test_float_separator_exponent_echo() {
    let out = compile_and_run("<?php echo 1e1_0;");
    assert_eq!(out, "10000000000");
}
