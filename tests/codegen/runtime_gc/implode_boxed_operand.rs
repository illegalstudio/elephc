//! Purpose:
//! Regression tests for issue #689's ownership half: `implode()` on a boxed array operand
//! hands the join site an OWNED array in both of its accepting branches, so that the release
//! that follows needs no run-time flag.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - The two branches reach owned-ness differently: a hash is CONVERTED to a fresh values
//!   array, and an indexed array is RETAINED. Both are released once by the join site.
//! - Each fixture runs under `--heap-debug` and asserts `leak summary: clean`. A loop is what
//!   makes an imbalance accumulate; a single call hides either direction of it.

use crate::support::compile_and_run_with_heap_debug;

/// Asserts the program printed `expected` and left a clean heap under heap debug.
fn assert_clean(out: crate::support::ProgramOutput, expected: &str) {
    assert_eq!(out.stdout, expected, "stderr: {}", out.stderr);
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean heap, got: {}",
        out.stderr
    );
}

/// Verifies the retained indexed payload is released exactly once per join.
///
/// The payload is BORROWED from the Mixed cell, so the lowering takes a reference to make the
/// join site's release unconditional. Getting that pair wrong leaks one array per call in one
/// direction and frees the caller's array in the other.
#[test]
fn test_implode_on_a_boxed_indexed_array_is_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function f(?array $a): string { return implode(',', $a); }
$ints = [1, 2, 3];
$floats = [1.5, 2.5];
$bools = [true, false];
$strs = ["a", "b"];
$n = 0;
for ($i = 0; $i < 64; $i++) {
    $n += strlen(f($ints)) + strlen(f($floats)) + strlen(f($bools)) + strlen(f($strs));
}
echo $n;
"#,
    );
    assert_clean(out, "1088");
}

/// Verifies the values array converted from a boxed HASH is released exactly once per join.
///
/// This copy is freshly allocated rather than retained, and it holds boxed cells of its own —
/// the leak it would produce is larger than one block per call.
#[test]
fn test_implode_on_a_boxed_associative_array_is_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function f(?array $a): string { return implode(',', $a); }
function g(mixed $a): string { return implode('|', $a); }
$hash = ["k" => 1, "j" => "s", "m" => 2.5];
$n = 0;
for ($i = 0; $i < 64; $i++) {
    $n += strlen(f($hash)) + strlen(g($hash));
}
echo $n;
"#,
    );
    assert_clean(out, "896");
}

/// Growing `implode()` past the concat scratch produces an owned heap string. Persisting the
/// result for a caller must transfer or release that allocation instead of leaking a duplicate.
#[test]
fn test_implode_large_retained_result_leaves_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$joined = implode('', array_fill(0, 7000, '0123456789'));
echo strlen($joined);
"#,
    );
    assert_clean(out, "70000");
}
