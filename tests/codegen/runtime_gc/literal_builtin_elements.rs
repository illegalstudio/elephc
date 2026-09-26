//! Purpose:
//! Ownership tests for issue #1096: an array literal whose element is an array-returning builtin
//! call now stamps the element as a heap value instead of `int`, so the outer literal owns a
//! refcounted element it previously did not — the release path has to match.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Each fixture runs under `--heap-debug` and asserts `leak summary: clean`.
//! - The loop count is high enough that a per-iteration leak of one block is unmistakable, and
//!   the sources are `$i`-dependent so no literal is folded away.
//! - Before the fix these programs were not leak-free-but-wrong: the element was cast to `int`,
//!   so the inner array was released immediately and nothing was retained. The stamp change is
//!   what puts a heap element inside the literal, which is why these are ownership tests.

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

/// The issue's own shape, in a loop: two `array_slice()` results collected into one literal.
/// Each iteration allocates two inner arrays that the outer literal now owns.
#[test]
fn test_literal_of_array_slice_elements_is_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$total = 0;
for ($i = 0; $i < 40; $i++) {
    $a = [$i, $i + 1, $i + 2, $i + 3];
    $c = [array_slice($a, 0, 2), array_slice($a, 2)];
    $total = $total + count($c[0]) + count($c[1]);
}
echo $total, "\n";
"#,
    );
    assert_clean(out, "160\n");
}

/// A string-valued inner array, whose elements are themselves heap blocks — the outer literal
/// owning the inner array has to keep them alive, and release them exactly once.
#[test]
fn test_literal_of_explode_elements_is_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$total = 0;
for ($i = 0; $i < 40; $i++) {
    $c = [explode(",", "a,b,c"), explode("|", "d|e")];
    $total = $total + count($c[0]) + count($c[1]);
}
echo $total, "\n";
"#,
    );
    assert_clean(out, "200\n");
}

/// The associative twin: the hash takes the inner array as its value, and `hash_set` steals it.
#[test]
fn test_assoc_literal_of_a_builtin_array_value_is_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$total = 0;
for ($i = 0; $i < 40; $i++) {
    $a = [$i, $i + 1, $i + 2];
    $c = ["head" => array_slice($a, 0, 1), "tail" => array_slice($a, 1)];
    $total = $total + count($c["head"]) + count($c["tail"]);
}
echo $total, "\n";
"#,
    );
    assert_clean(out, "120\n");
}

/// The literal escapes the iteration into a longer-lived local, so the release happens on the
/// rebind rather than at the end of the statement — the other side of the same ownership edge.
#[test]
fn test_literal_of_builtin_arrays_rebound_across_iterations_is_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$c = [];
$total = 0;
for ($i = 0; $i < 40; $i++) {
    $a = [$i, $i + 1, $i + 2];
    $c = [array_slice($a, 0, 2)];
    $total = $total + count($c[0]);
}
echo $total, "\n";
"#,
    );
    assert_clean(out, "80\n");
}
