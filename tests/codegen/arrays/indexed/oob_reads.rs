//! Purpose:
//! Regression tests for type-aware null fallback on out-of-bounds indexed reads
//! and associative-array misses.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Before the fix, the miss path set only the integer result register, leaving
//!   the string (ptr/len) or float result registers holding STALE values from a
//!   prior expression — so an out-of-bounds Str read re-emitted previously-echoed
//!   bytes and a Float read returned a stale value. These tests pin the corrected
//!   behavior: OOB/missing Str → empty string, Float → 0.0, with no duplicated
//!   output and no crash.

use crate::support::*;

/// An out-of-bounds read on a string array must not re-emit stale bytes from a
/// prior expression: `echo $a[oob]` produces nothing (empty string), so the
/// surrounding markers print adjacently.
#[test]
fn test_indexed_oob_str_read_is_empty_not_stale() {
    let out = compile_and_run(
        r#"<?php
$a = ["hello", "world"];
echo "P";
echo $a[9];
echo "Q";
"#,
    );
    assert_eq!(out, "PQ");
}

/// A float array out-of-bounds read (inside a function so the element type is a
/// concrete Float local) returns a deterministic 0.0, not a stale prior float.
#[test]
fn test_indexed_oob_float_read_is_zero_not_stale() {
    let out = compile_and_run(
        r#"<?php
function pick(array $a, int $i): float { return $a[$i]; }
$b = [1.5, 2.5];
echo "X" . pick($b, 9) . "Y";
"#,
    );
    assert_eq!(out, "X0Y");
}

/// Iterating a string array and THEN reading out of bounds must not crash and
/// must print an empty slot (the historical SIGSEGV/garbage-byte case).
#[test]
fn test_indexed_oob_str_after_iteration_no_crash() {
    let out = compile_and_run(
        r#"<?php
$a = ["hello", "world"];
foreach ($a as $w) { echo $w; }
echo "[" . $a[9] . "]";
"#,
    );
    assert_eq!(out, "helloworld[]");
}

/// A missing string-valued associative key (via the null-coalescing default)
/// yields an empty string, with the surrounding markers adjacent.
#[test]
fn test_assoc_str_miss_is_empty() {
    let out = compile_and_run(
        r#"<?php
$m = ["a" => "FIRST"];
echo "R" . ($m["zzz"] ?? "") . "S";
"#,
    );
    assert_eq!(out, "RS");
}

/// A negative index on a string array also takes the null-fallback path and
/// must yield an empty string without re-emitting stale bytes.
#[test]
fn test_indexed_negative_str_read_is_empty() {
    let out = compile_and_run(
        r#"<?php
$a = ["alpha", "beta"];
echo "before";
echo $a[-1];
echo "after";
"#,
    );
    assert_eq!(out, "beforeafter");
}

/// Regression for #350: null coalescing on an empty array with a string key.
#[test]
fn test_empty_array_string_key_null_coalesce() {
    let out = compile_and_run("<?php $a = []; echo ($a['k'] ?? 'x');");
    assert_eq!(out, "x");
}

/// Regression for #350: isset() on an empty array with a string key.
#[test]
fn test_empty_array_string_key_isset() {
    let out = compile_and_run("<?php $a = []; echo isset($a['k']) ? 'yes' : 'no';");
    assert_eq!(out, "no");
}

/// Regression for #350: unset() on an empty array with a string key.
#[test]
fn test_empty_array_string_key_unset() {
    let out = compile_and_run("<?php $a = []; unset($a['k']); echo 'ok';");
    assert_eq!(out, "ok");
}

/// Regression for #350: numeric-string key under ?? on an empty array.
#[test]
fn test_empty_array_numeric_string_key_coalesce() {
    let out = compile_and_run("<?php $a = []; echo ($a['1'] ?? 'x');");
    assert_eq!(out, "x");
}

/// Regression for #361: missing string-key ?? lookup on an associative array.
#[test]
fn test_missing_string_key_null_coalesce() {
    let out = compile_and_run("<?php $a = []; echo $a['missing'] ?? 'ok';");
    assert_eq!(out, "ok");
}

/// Regression for #350/#361: string-key access followed by unset on an empty array.
#[test]
fn test_coalesce_then_unset_empty_array() {
    let out = compile_and_run(r#"<?php
$a = [];
echo ($a['k'] ?? 'x');
echo "\n";
echo isset($a['k']) ? 'yes' : 'no';
echo "\n";
unset($a['k']);
echo 'ok';
"#);
    assert_eq!(out, "x\nno\nok");
}
