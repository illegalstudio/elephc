//! Purpose:
//! Heap-debug coverage for `array_chunk()` over an associative receiver. Each chunk becomes a
//! second owner of every value it copies, so `__rt_hash_chunk` takes a reference per entry —
//! persisting strings into independent heap blocks and retaining heap-backed values — and those
//! references have to come back when the chunks are released.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Each fixture runs under `--heap-debug` and asserts `leak summary: clean`. Retaining without
//!   a matching release leaks one payload per entry; releasing without the retain frees a value
//!   the chunk still points at, which shows up as a corrupted read rather than a leak — so the
//!   fixtures read the copied values back as well.
//! - The loops copy hundreds of payloads, so a per-entry leak cannot hide in heap slack.
//! - Expected stdout values are real `LC_ALL=C php` 8.5 output for the same fixtures.

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

/// String values are persisted per chunk and released with it, in both key modes.
#[test]
fn test_assoc_chunk_string_values_balance() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$t = 0;
for ($i = 0; $i < 200; $i++) {
    $src = ["a" => "one", "b" => "two", "c" => "three"];
    $k = array_chunk($src, 2, true);
    $t += count($k) + strlen($k[0]["a"]);
    unset($k);
    $r = array_chunk($src, 2);
    $t += count($r) + strlen($r[1][0]);
    unset($r);
    unset($src);
}
echo $t;
"#,
    );
    assert_clean(out, "2400");
}

/// Heap-backed values are retained per chunk and released with it.
///
/// The source is dropped before the chunks are read, so an over-release would surface as a
/// corrupted count rather than as a leak.
#[test]
fn test_assoc_chunk_nested_array_values_balance() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$t = 0;
for ($i = 0; $i < 200; $i++) {
    $src = ["p" => [1, 2], "q" => [3], "r" => [4, 5, 6]];
    $c = array_chunk($src, 2, true);
    unset($src);
    $t += count($c[0]["p"]) + count($c[1]["r"]);
    unset($c);
}
echo $t;
"#,
    );
    assert_clean(out, "1000");
}

/// Scalar values are copied by value, so the chunks own no extra references at all.
#[test]
fn test_assoc_chunk_scalar_values_balance() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$t = 0;
for ($i = 0; $i < 200; $i++) {
    $src = ["a" => 1, "b" => 2, "c" => 3, "d" => 4, "e" => 5];
    $c = array_chunk($src, 2, true);
    $t += count($c) + $c[2]["e"];
    unset($c);
    unset($src);
}
echo $t;
"#,
    );
    assert_clean(out, "1600");
}
