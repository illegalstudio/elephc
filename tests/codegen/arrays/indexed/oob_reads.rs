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
