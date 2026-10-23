//! Purpose:
//! Codegen tests for PHP's `random_bytes()`.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - The bytes are random, so the fixtures check lengths, distinctness and PHP's
//!   `ValueError`, and that the owned result strings do not leak.

use crate::support::*;

/// `random_bytes()` returns the requested number of bytes from the OS generator.
#[test]
fn test_random_bytes_lengths_and_distinct_values() {
    let out = compile_and_run(
        r#"<?php
$a = random_bytes(16);
$b = random_bytes(16);
echo strlen($a), ",", $a !== $b ? "distinct" : "same", ",", strlen(bin2hex(random_bytes(1))), ",",
    strlen(random_bytes(100000)), "\n";
$f = 'random_bytes';
echo strlen($f(5)), strlen(call_user_func('random_bytes', 7)), "\n";
eval('echo strlen(random_bytes(8)), "\n";');
"#,
    );
    assert_eq!(out, "16,distinct,2,100000\n57\n8\n");
}

/// A `$length` below 1 is php-src's catchable `ValueError`, compiled and in eval.
#[test]
fn test_random_bytes_rejects_non_positive_length() {
    let out = compile_and_run(
        r#"<?php
foreach ([0, -1] as $n) {
    try { random_bytes($n); } catch (ValueError $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
}
eval('try { random_bytes(0); } catch (\ValueError $e) { echo $e->getMessage(), "\n"; }');
"#,
    );
    assert_eq!(
        out,
        "ValueError: random_bytes(): Argument #1 ($length) must be greater than 0\n\
         ValueError: random_bytes(): Argument #1 ($length) must be greater than 0\n\
         random_bytes(): Argument #1 ($length) must be greater than 0\n"
    );
}

/// A `null` `$length` still compiles in PHP's coercive mode: it converts to `0` and raises
/// the catchable `ValueError`, directly and through a nullable variable.
#[test]
fn test_random_bytes_null_length_is_a_catchable_value_error() {
    let out = compile_and_run(
        r#"<?php
try { random_bytes(null); } catch (ValueError $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
$n = $argc > 5 ? 4 : null;
try { echo strlen(random_bytes($n)), "\n"; } catch (ValueError $e) { echo $e->getMessage(), "\n"; }
"#,
    );
    assert_eq!(
        out,
        "ValueError: random_bytes(): Argument #1 ($length) must be greater than 0\n\
         random_bytes(): Argument #1 ($length) must be greater than 0\n"
    );
}

/// A string `$length` follows PHP's weak-mode `int` parameter: numeric strings (with
/// surrounding whitespace, or a float form that truncates) are lengths, anything else is the
/// catchable `TypeError`, compiled and in eval.
#[test]
fn test_random_bytes_numeric_string_lengths() {
    let out = compile_and_run(
        r#"<?php
foreach (["16", " 4", "4 ", "2.0", "3abc", "abc", ""] as $s) {
    try { echo strlen(random_bytes($s)), "\n"; } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
}
echo strlen(random_bytes("8")), "\n";
eval('echo strlen(random_bytes(" 1e1 ")), "\n"; try { random_bytes("3abc"); } catch (\TypeError $e) { echo $e->getMessage(), "\n"; }');
"#,
    );
    assert_eq!(
        out,
        "16\n4\n4\n2\n\
         random_bytes(): Argument #1 ($length) must be of type int, string given\n\
         random_bytes(): Argument #1 ($length) must be of type int, string given\n\
         random_bytes(): Argument #1 ($length) must be of type int, string given\n8\n\
         10\n\
         random_bytes(): Argument #1 ($length) must be of type int, string given\n"
    );
}

/// `random_bytes()` resolves case-insensitively and from inside a namespace (global fallback).
#[test]
fn test_random_bytes_case_insensitive_and_namespaced_calls() {
    let out = compile_and_run(
        r#"<?php
namespace App\Tokens;
echo strlen(RANDOM_BYTES(3)), strlen(Random_Bytes(2)), strlen(\random_bytes(4)), strlen(random_bytes(5)), "\n";
"#,
    );
    assert_eq!(out, "3245\n");
}

/// The fresh result strings are released like any other owned string.
#[test]
fn test_random_bytes_results_do_not_leak() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$total = 0;
for ($i = 0; $i < 200; $i++) {
    $bytes = random_bytes(32);
    $total += strlen($bytes);
    random_bytes(3);
}
echo $total, "\n";
"#,
    );
    assert!(out.success, "stderr: {}", out.stderr);
    assert_eq!(out.stdout, "6400\n");
    assert!(out.stderr.contains("leak summary: clean"), "{}", out.stderr);
}
