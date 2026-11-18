//! Purpose:
//! Regression tests for loop accumulators seeded with an int that turn float inside the loop.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Every expectation here is the host PHP 8.5 (`php -n`) output for the same fixture.
//! - The loop-storage contract may treat `int + int` as a checked update of an int local, but an
//!   int local that picks up a float operand changes its storage and must not keep a raw int slot.

use super::*;

/// An int-seeded accumulator that becomes float through division, overflow, or a float operand.
#[test]
fn test_int_seeded_loop_accumulator_turning_float_matches_php() {
    let out = compile_and_run(
        r#"<?php
$a = 0; for ($j = 0; $j < 4; $j++) { $a = $a + PHP_INT_MAX / 2 + 1; } var_dump($a);
$b = 1; for ($j = 0; $j < 3; $j++) { $b = $b / 2; } var_dump($b);
$c = PHP_INT_MAX - 2; for ($j = 0; $j < 4; $j++) { $c = $c + 1; } var_dump($c);
$d = 1; for ($j = 0; $j < 70; $j++) { $d = $d * 2; } var_dump($d);
$f = 0; for ($j = 0; $j < 3; $j++) { $f = $f + 0.5; } var_dump($f);
$g = 7; for ($j = 0; $j < 3; $j++) { $g = $g - 1.5 * $j; } var_dump($g);
$h = 0; $k = 1.5; for ($j = 0; $j < 3; $j++) { $h += $k; } var_dump($h);
"#,
    );
    assert_eq!(
        out,
        "float(1.8446744073709552E+19)\nfloat(0.125)\nfloat(9.223372036854776E+18)\nfloat(1.1805916207174113E+21)\nfloat(1.5)\nfloat(2.5)\nfloat(4.5)\n"
    );
}

/// Float seeds stay float and int seeds that stay int keep their exact value.
#[test]
fn test_float_and_int_loop_accumulators_keep_their_type() {
    let out = compile_and_run(
        r#"<?php
$e = 0.5; for ($j = 0; $j < 3; $j++) { $e = $e + 1; } var_dump($e);
$m = 3; for ($j = 0; $j < 3; $j++) { $m = $m * 2 + 1; } var_dump($m);
$n = 0; for ($j = 0; $j < 5; $j++) { $n = $n + $j; } var_dump($n);
"#,
    );
    assert_eq!(out, "float(3.5)\nint(31)\nint(10)\n");
}
