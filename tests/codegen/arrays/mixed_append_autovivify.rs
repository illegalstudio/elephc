//! Purpose:
//! Regression coverage for appending (`$r[] = v`) to a boxed `Mixed` cell whose
//! payload is a null container: the in-band null-container sentinel produced by
//! a missed indexed read forwarded through a ternary/branch merge, a null
//! pointer payload, or an unset/null cell (issue #592). PHP auto-vivifies such
//! a null into a fresh single-element array; elephc previously dereferenced the
//! sentinel inline and segfaulted (exit 139).
//!
//! Called from:
//! - `cargo test --test codegen_tests arrays::mixed_append_autovivify`.
//!
//! Key details:
//! - Before the fix, `lower_mixed_array_append` unboxed the receiver, accepted a
//!   tag-4 (indexed) payload, and read the array header inline (`ldur x1,
//!   [x0, #-8]`) before calling `__rt_array_to_mixed`. A missed-read sentinel
//!   (`0x7ffffffffffffffe`) passed the plain-zero guard and the header read
//!   faulted at `sentinel - 8`.
//! - The fix guards the payload for the null pointer and the null-container
//!   sentinel, normalizes null-shaped containers to canonical Mixed null while
//!   boxing, and delegates every append to `__rt_mixed_array_append`. The runtime
//!   helper autovivifies null, writes indexed arrays through the shared setter,
//!   and appends real associative arrays through `__rt_hash_append`.
//! - The fixtures make the taken arm `$argc`-dependent so the ternary genuinely
//!   merges to `Mixed` (constant folding cannot collapse it); the test harness
//!   runs the binary with no arguments, so `$argc == 1` selects the first arm.
//! - Heap-debug fixtures assert a clean heap: autovivification must not leak the
//!   fresh array, its boxed cell, or the appended value. Readbacks use index
//!   access and `count()` (not `implode`, which has an unrelated Mixed-array
//!   retention issue) so the assertions isolate the append path.

use crate::support::{compile_and_run, compile_and_run_with_heap_debug};

/// Issue #592 exact repro: the missed read `$rows[5]` boxes the null-container
/// sentinel into `$r`, then `$r[] = "z"` must auto-vivify `["z"]` and the
/// program must print `done` instead of segfaulting.
#[test]
fn test_append_autovivify_exact_issue_repro() {
    let out = compile_and_run(
        r#"<?php
$rows = [[1, 2]];
$r = $argc == 1 ? $rows[5] : ["a", "b"];
$r[] = "z";
echo "done", "\n";
"#,
    );
    assert_eq!(out, "done\n");
}

/// The auto-vivified array must be exactly `["z"]`: length 1 with the appended
/// value at index 0 (PHP `$r === ["z"]`).
#[test]
fn test_append_autovivifies_missed_read_sentinel() {
    let out = compile_and_run(
        r#"<?php
$rows = [[1, 2]];
$r = $argc == 1 ? $rows[5] : ["a", "b"];
$r[] = "z";
echo count($r), ":", $r[0], "\n";
"#,
    );
    assert_eq!(out, "1:z\n");
}

/// Heap-debug variant: the sentinel append must auto-vivify with a clean heap
/// and still emit PHP's undefined-key warning for the missed read on stderr.
#[test]
fn test_append_autovivify_sentinel_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$rows = [[1, 2]];
$r = $argc == 1 ? $rows[5] : ["a", "b"];
$r[] = "z";
echo count($r), ":", $r[0], "\n";
"#,
    );
    assert!(out.success, "program must exit successfully, stderr: {}", out.stderr);
    assert_eq!(out.stdout, "1:z\n");
    assert!(
        out.stderr.contains("Undefined array key 5"),
        "the missed read must still warn like PHP, got: {}",
        out.stderr
    );
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "auto-vivification must not leak, got: {}",
        out.stderr
    );
}

/// Associative sibling: a missed read on an array of hashes also boxes the
/// null-container sentinel; the append auto-vivifies a fresh indexed array and
/// stays heap-clean.
#[test]
fn test_append_autovivifies_assoc_missed_read() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$rows = [["x" => 1]];
$r = $argc == 1 ? $rows[5] : ["a" => 1];
$r[] = "z";
echo count($r), ":", $r[0], "\n";
"#,
    );
    assert!(out.success, "program must exit successfully, stderr: {}", out.stderr);
    assert_eq!(out.stdout, "1:z\n");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "auto-vivification must not leak, got: {}",
        out.stderr
    );
}

/// Multiple appends onto the auto-vivified array must grow it correctly (index
/// tracking and reallocation), producing `[10, 20, 30]` with a clean heap.
#[test]
fn test_append_autovivify_multi_grows_array() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$rows = [[1, 2]];
$r = $argc == 1 ? $rows[5] : ["a"];
$r[] = 10;
$r[] = 20;
$r[] = 30;
echo count($r), ":", $r[0], ",", $r[2], "\n";
"#,
    );
    assert!(out.success, "program must exit successfully, stderr: {}", out.stderr);
    assert_eq!(out.stdout, "3:10,30\n");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "auto-vivification must not leak, got: {}",
        out.stderr
    );
}

/// Control: a `Mixed` cell that holds a REAL indexed array (mismatched-type
/// ternary arms widen to `Mixed`, and the taken arm is a genuine array) must
/// still append through the normal in-place path — no double conversion, no
/// autovivify, no undefined-key warning, and no leak.
#[test]
fn test_append_real_array_mixed_cell_control() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$r = $argc == 1 ? ["a", "b"] : [1, 2];
$r[] = "z";
echo $r[0], $r[1], $r[2], ":", count($r), "\n";
"#,
    );
    assert!(out.success, "program must exit successfully, stderr: {}", out.stderr);
    assert_eq!(out.stdout, "abz:3\n");
    assert!(
        !out.stderr.contains("Undefined array key"),
        "a real-array append must not warn, got: {}",
        out.stderr
    );
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "the real-array append path must not leak, got: {}",
        out.stderr
    );
}

/// A canonical tag-8 Mixed null (not a missed-read sentinel) must exercise the
/// same PHP autovivification contract and transfer the fresh array cleanly.
#[test]
fn test_append_autovivifies_canonical_mixed_null() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function append_to_null(mixed $value): mixed {
    $value[] = "z";
    return $value;
}
$r = append_to_null(null);
echo count($r), ":", $r[0], "\n";
"#,
    );
    assert!(out.success, "program must exit successfully, stderr: {}", out.stderr);
    assert_eq!(out.stdout, "1:z\n");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "canonical-null autovivification must not leak, got: {}",
        out.stderr
    );
}

/// A real associative array inside a Mixed cell must preserve its string and
/// integer keys while `$r[]` uses PHP's next automatic integer key.
#[test]
fn test_append_real_assoc_mixed_cell_uses_next_integer_key() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$r = $argc == 1 ? ["x" => 1, 5 => "five"] : 7;
$r[] = "z";
echo count($r), ":", $r["x"], ":", $r[5], ":", $r[6], "\n";
"#,
    );
    assert!(out.success, "program must exit successfully, stderr: {}", out.stderr);
    assert_eq!(out.stdout, "3:1:five:z\n");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "associative Mixed append must not leak, got: {}",
        out.stderr
    );
}
