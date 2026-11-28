//! Purpose:
//! End-to-end coverage for the constant-divisor `%` fast path in integer lowering.
//!
//! Called from:
//! - The codegen optimizer suite through `optimizer`.
//!
//! Key details:
//! - PHP truncates integer division toward zero, so the remainder follows the dividend's sign.
//! - Divisor `1` and `-1` always produce zero; any other non-zero constant skips the runtime
//!   zero-divisor guard while a variable divisor keeps it.
//! - Loop counters make the dividend runtime-unknown, so the fold cannot precompute the result.

use crate::support::*;

/// Constant and variable divisors match PHP for negative and positive dividends.
#[test]
fn test_constant_modulo_matches_php() {
    let source = r#"<?php
$start = -$argc - 6;
$end = $argc + 6;
$out = [];
for ($i = $start; $i <= $end; $i++) {
    $out[] = $i % 3;
}
for ($i = $start; $i <= $end; $i++) {
    $out[] = $i % -3;
}
for ($i = $start; $i <= $end; $i++) {
    $out[] = $i % 1;
}
for ($i = $start; $i <= $end; $i++) {
    $out[] = $i % -1;
}
$d = $argc + 2;
for ($i = $start; $i <= $end; $i++) {
    $out[] = $i % $d;
}
echo implode(",", $out), "\n";
"#;
    let expected = "-1,0,-2,-1,0,-2,-1,0,1,2,0,1,2,0,1,-1,0,-2,-1,0,-2,-1,0,1,2,0,1,2,0,1,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,-1,0,-2,-1,0,-2,-1,0,1,2,0,1,2,0,1\n";
    assert_eq!(compile_and_run(source), expected);
}

/// `PHP_INT_MIN % -1` is zero even though the matching division overflows.
#[test]
fn test_min_int_modulo_negative_one_is_zero() {
    assert_eq!(
        compile_and_run("<?php $min = PHP_INT_MIN + ($argc - 1); echo $min % -1, \"\\n\";"),
        "0\n"
    );
}

/// A variable divisor still raises `DivisionByZeroError` at runtime.
#[test]
fn test_variable_zero_modulo_still_throws() {
    let source = r#"<?php
$zero = $argc - 1;
try {
    echo 5 % $zero;
} catch (DivisionByZeroError $e) {
    echo get_class($e), ":", $e->getMessage(), "\n";
}
"#;
    assert_eq!(
        compile_and_run(source),
        "DivisionByZeroError:Modulo by zero\n"
    );
}
