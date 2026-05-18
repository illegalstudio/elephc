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

#[test]
fn test_echo_float() {
    let out = compile_and_run("<?php echo 3.14;");
    assert_eq!(out, "3.14");
}

#[test]
fn test_echo_float_integer_value() {
    let out = compile_and_run("<?php echo 4.0;");
    assert_eq!(out, "4");
}

#[test]
fn test_echo_negative_float() {
    let out = compile_and_run("<?php echo -3.14;");
    assert_eq!(out, "-3.14");
}

#[test]
fn test_echo_dot_prefix_float() {
    let out = compile_and_run("<?php echo .5;");
    assert_eq!(out, "0.5");
}

// --- Float arithmetic ---

#[test]
fn test_float_addition() {
    let out = compile_and_run("<?php echo 1.5 + 2.3;");
    assert_eq!(out, "3.8");
}

#[test]
fn test_float_subtraction() {
    let out = compile_and_run("<?php echo 5.5 - 2.2;");
    assert_eq!(out, "3.3");
}

#[test]
fn test_float_multiplication() {
    let out = compile_and_run("<?php echo 3.0 * 2.5;");
    assert_eq!(out, "7.5");
}

#[test]
fn test_float_division() {
    let out = compile_and_run("<?php echo 7.5 / 2.5;");
    assert_eq!(out, "3");
}

// --- Mixed int+float ---

#[test]
fn test_int_plus_float() {
    let out = compile_and_run("<?php echo 10 + 0.5;");
    assert_eq!(out, "10.5");
}

#[test]
fn test_float_plus_int() {
    let out = compile_and_run("<?php echo 0.5 + 10;");
    assert_eq!(out, "10.5");
}

#[test]
fn test_int_times_float() {
    let out = compile_and_run("<?php echo 3 * 1.5;");
    assert_eq!(out, "4.5");
}

// --- Float comparison ---

#[test]
fn test_float_greater_than() {
    let out = compile_and_run("<?php echo 3.14 > 2.0;");
    assert_eq!(out, "1");
}

#[test]
fn test_float_less_than() {
    let out = compile_and_run("<?php echo 1.5 < 2.5;");
    assert_eq!(out, "1");
}

#[test]
fn test_float_equal() {
    let out = compile_and_run("<?php echo 3.14 == 3.14;");
    assert_eq!(out, "1");
}

#[test]
fn test_float_not_equal() {
    let out = compile_and_run("<?php echo 3.14 != 2.0;");
    assert_eq!(out, "1");
}

// --- Float concatenation ---

#[test]
fn test_float_concat() {
    let out = compile_and_run("<?php echo \"pi=\" . 3.14;");
    assert_eq!(out, "pi=3.14");
}

#[test]
fn test_float_concat_reverse() {
    let out = compile_and_run("<?php echo 3.14 . \" is pi\";");
    assert_eq!(out, "3.14 is pi");
}

// --- Math functions ---

#[test]
fn test_floor() {
    let out = compile_and_run("<?php echo floor(3.7);");
    assert_eq!(out, "3");
}

#[test]
fn test_ceil() {
    let out = compile_and_run("<?php echo ceil(3.2);");
    assert_eq!(out, "4");
}

#[test]
fn test_round() {
    let out = compile_and_run("<?php echo round(3.5);");
    assert_eq!(out, "4");
}

#[test]
fn test_round_down() {
    let out = compile_and_run("<?php echo round(3.4);");
    assert_eq!(out, "3");
}

#[test]
fn test_sqrt() {
    let out = compile_and_run("<?php echo sqrt(16.0);");
    assert_eq!(out, "4");
}

#[test]
fn test_sqrt_non_perfect() {
    let out = compile_and_run("<?php echo sqrt(2.0);");
    assert_eq!(out, "1.4142135623731");
}

#[test]
fn test_abs_float() {
    let out = compile_and_run("<?php echo abs(-3.14);");
    assert_eq!(out, "3.14");
}

#[test]
fn test_abs_int() {
    let out = compile_and_run("<?php echo abs(-42);");
    assert_eq!(out, "42");
}

#[test]
fn test_pow() {
    let out = compile_and_run("<?php echo pow(2.0, 10.0);");
    assert_eq!(out, "1024");
}

#[test]
fn test_min_int() {
    let out = compile_and_run("<?php echo min(3, 7);");
    assert_eq!(out, "3");
}

#[test]
fn test_max_int() {
    let out = compile_and_run("<?php echo max(3, 7);");
    assert_eq!(out, "7");
}

#[test]
fn test_min_float() {
    let out = compile_and_run("<?php echo min(1.5, 2.5);");
    assert_eq!(out, "1.5");
}

#[test]
fn test_max_float() {
    let out = compile_and_run("<?php echo max(1.5, 2.5);");
    assert_eq!(out, "2.5");
}

#[test]
fn test_intdiv() {
    let out = compile_and_run("<?php echo intdiv(7, 2);");
    assert_eq!(out, "3");
}

// --- Type checking builtins ---

#[test]
fn test_floatval() {
    let out = compile_and_run("<?php echo floatval(42);");
    assert_eq!(out, "42");
}

#[test]
fn test_is_float_true() {
    let out = compile_and_run("<?php echo is_float(3.14);");
    assert_eq!(out, "1");
}

#[test]
fn test_is_float_false() {
    let out = compile_and_run("<?php echo is_float(42);");
    assert_eq!(out, "");
}

#[test]
fn test_is_int_true() {
    let out = compile_and_run("<?php echo is_int(42);");
    assert_eq!(out, "1");
}

#[test]
fn test_is_int_false() {
    let out = compile_and_run("<?php echo is_int(3.14);");
    assert_eq!(out, "");
}

// --- Float variable ---

#[test]
fn test_float_variable() {
    let out = compile_and_run("<?php $x = 3.14; echo $x;");
    assert_eq!(out, "3.14");
}

#[test]
fn test_float_variable_arithmetic() {
    let out = compile_and_run("<?php $a = 1.5; $b = 2.5; echo $a + $b;");
    assert_eq!(out, "4");
}

#[test]
fn test_float_in_condition() {
    let out =
        compile_and_run("<?php $x = 3.14; if ($x > 3.0) { echo \"yes\"; } else { echo \"no\"; }");
    assert_eq!(out, "yes");
}

// --- Large integer literal promotion ---

#[test]
fn test_max_integer_literal_stays_integer() {
    let out = compile_and_run(
        "<?php echo gettype(9223372036854775807) . \"|\" . gettype(0x7FFFFFFFFFFFFFFF);",
    );
    assert_eq!(out, "integer|integer");
}

#[test]
fn test_large_decimal_integer_literal_promotes_to_float() {
    let out = compile_and_run("<?php echo gettype(9223372036854775808);");
    assert_eq!(out, "double");
}

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

#[test]
fn test_octal_literal_echo() {
    let out = compile_and_run("<?php echo 0o777;");
    assert_eq!(out, "511");
}

#[test]
fn test_legacy_octal_literal_echo() {
    let out = compile_and_run("<?php echo 0777;");
    assert_eq!(out, "511");
}

#[test]
fn test_legacy_octal_separator_echo() {
    let out = compile_and_run("<?php echo 0_777;");
    assert_eq!(out, "511");
}

#[test]
fn test_leading_zero_float_is_decimal() {
    let out = compile_and_run("<?php echo 08.5;");
    assert_eq!(out, "8.5");
}

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

#[test]
fn test_binary_literal_echo() {
    let out = compile_and_run("<?php echo 0b1010;");
    assert_eq!(out, "10");
}

#[test]
fn test_binary_literal_arith() {
    let out = compile_and_run("<?php echo 0b1100 & 0b1010;");
    assert_eq!(out, "8");
}

#[test]
fn test_binary_literal_uppercase() {
    let out = compile_and_run("<?php echo 0B11111111;");
    assert_eq!(out, "255");
}

// --- Numeric separators (PHP 7.4+) ---

#[test]
fn test_decimal_separator_echo() {
    let out = compile_and_run("<?php echo 1_000_000;");
    assert_eq!(out, "1000000");
}

#[test]
fn test_hex_separator_echo() {
    let out = compile_and_run("<?php echo 0xFF_FF;");
    assert_eq!(out, "65535");
}

#[test]
fn test_octal_separator_echo() {
    let out = compile_and_run("<?php echo 0o7_7_7;");
    assert_eq!(out, "511");
}

#[test]
fn test_binary_separator_echo() {
    let out = compile_and_run("<?php echo 0b1010_1010;");
    assert_eq!(out, "170");
}

#[test]
fn test_float_separator_echo() {
    let out = compile_and_run("<?php echo 1_000.5;");
    assert_eq!(out, "1000.5");
}

#[test]
fn test_float_separator_exponent_echo() {
    let out = compile_and_run("<?php echo 1e1_0;");
    assert_eq!(out, "10000000000");
}
