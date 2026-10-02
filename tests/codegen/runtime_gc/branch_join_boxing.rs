//! Purpose:
//! Heap-debug regression tests for issue #771: locals joined across divergent `if` arms are boxed
//! at the merge, and every value they held along the way must still be released.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Each fixture runs under `--heap-debug` in a loop and asserts `leak summary: clean`, so a
//!   per-call leak shows up as a non-clean summary instead of hiding in the noise of one call.
//! - Expected output was produced by PHP 8.5.

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

/// The #771 repro in a loop: the edge-joined capture releases every string it held.
#[test]
fn test_closure_local_retype_branch_join_heap_clean_string_capture() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$m = "k" . $argc;
$f = function (int $n) use ($m) { $m = null; if ($n > 1) { $m = "s" . $n; } return $m; };
$total = 0;
for ($i = 0; $i < 200; $i++) { $total += strlen((string) $f($i % 3)); }
echo $total, "\n";
"#,
    );
    assert_clean(out, "132\n");
}

/// A `null`-or-int local is boxed on each merge edge; the boxes the backend made for the earlier
/// scalar stores must be released by the deferred slot release, not leaked once per call.
#[test]
fn test_closure_local_retype_branch_join_heap_clean_scalar_word_edge_boxing() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$d = function (int $n) { $v = null; if ($n % 2 === 0) { $v = $n; $v = $v + 1; } return $v; };
$w = function (int $n) { $v = 0; $v = null; if ($n % 3 === 0) { $v = true; } return $v; };
$total = 0;
for ($i = 0; $i < 200; $i++) { $total += (int) $d($i) + ($w($i) === true ? 1 : 0); }
echo $total, "\n";
"#,
    );
    assert_clean(out, "10067\n");
}

/// Float/string, object and array arms joined inside a loop leave the heap clean.
#[test]
fn test_closure_local_retype_branch_join_heap_clean_mixed_arms() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$g = function (int $n) { $v = 1; if ($n % 3 === 1) { $v = 2.5; } elseif ($n % 3 === 2) { $v = "x" . $n; } return $v; };
$h = function (int $n) { $v = null; if ($n % 2 === 0) { $v = [$n, "e" . $n]; } return $v; };
$o = function (int $n) { $v = null; if ($n % 2 === 0) { $v = new ArrayObject([$n]); } return $v; };
$total = 0;
for ($i = 0; $i < 200; $i++) {
    $total += strlen((string) $g($i)) + count($h($i) ?? []) + count($o($i) ?? []);
}
echo $total, "\n";
"#,
    );
    assert_clean(out, "796\n");
}

/// A pointer builtin that read the array before the `if` loaded it as a concrete array; the
/// merge-edge box then widened the slot to Mixed, turning that load into an owned unbox that
/// must be released. A named function's `null` arm reading its object slot stays clean too.
#[test]
fn test_branch_join_heap_clean_pointer_call_before_join() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$f = function (int $n) { $h = ["a" => "x" . $n, "b" => "y" . $n]; end($h); if ($n > 500) { $h = null; } return key($h) . current($h); };
$g = function (int $n) { $a = [1, 2, 3]; next($a); if ($n > 500) { $a = null; } return current($a); };
function jo(int $n) { $o = new ArrayObject([$n]); if ($n % 2) { $o = null; } $r = $o instanceof ArrayObject ? 3 : 1; return $r; }
$total = 0;
for ($i = 0; $i < 200; $i++) { $total += strlen($f($i)) + $g($i) + jo($i); }
echo $total, "\n";
"#,
    );
    assert_clean(out, "1690\n");
}

/// A callable arm boxed on its merge edge BEFORE the slot is `Mixed` still releases the unbox's
/// reference: the first arm boxed here is the closure one, which leaked a descriptor per call.
#[test]
fn test_branch_join_heap_clean_callable_arm_boxed_first() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$g = function (int $n) {
    $c = function () { return 1; };
    if ($n > 0) { $c = function () { return 2; }; } else { $c = null; }
    if ($n > 1) { echo ""; }
    return $c === null ? 0 : $c();
};
$t = 0;
for ($i = 0; $i < 100; $i++) { $t += $g($i % 3); }
echo $t, "\n";
"#,
    );
    assert_clean(out, "132\n");
}

/// Expression arms joined like `if` arms, and a loop head boxing a `null`-entry local, release
/// every object they box or replace.
#[test]
fn test_lazy_expression_and_loop_entry_boxing_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$f = function (int $n) { $o = new ArrayObject([$n]); $x = $n % 2 ? ($o = null) : 0; $n % 3 && ($o = null); return $o === null ? -1 : count($o); };
$t = 0; for ($i = 0; $i < 100; $i++) { $t += $f($i); } echo $t, "\n";
function lh(int $n) { $o = new ArrayObject([$n]); $o = null; for ($i = 0; $i < 3; $i++) { if ($o !== null) { $n += count($o); } if ($i >= 0) { $o = new ArrayObject([$i, $n]); } } return $n; }
$t = 0; for ($i = 0; $i < 100; $i++) { $t += lh($i); } echo $t, "\n";
"#,
    );
    assert_clean(out, "-66\n5350\n");
}
