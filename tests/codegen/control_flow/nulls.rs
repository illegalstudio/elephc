//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of control flow nulls, including ternary null is falsy.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

/// Tests that `null` in a ternary condition is treated as falsy, matching PHP semantics.
/// Verifies that `$x = null; echo $x ? "yes" : "no";` outputs `"no"`.
#[test]
fn test_ternary_null_is_falsy() {
    let out = compile_and_run("<?php $x = null; echo $x ? \"yes\" : \"no\";");
    assert_eq!(out, "no");
}

/// Regression test for issue #549 (`??` sibling): a nullable array value side
/// (runtime boxed-Mixed elements from the `?array` return contract) merged
/// with an array<string> default must produce an array-of-Mixed temp. Before
/// the fix the default's element type won and the value side's boxed cells
/// were read back as string descriptors.
#[test]
fn test_null_coalesce_nullable_array_value_widens_string_default_merge() {
    let out = compile_and_run(
        r#"<?php
function f(int $n): ?array {
    if ($n === 1) {
        return [1, 2];
    }
    return null;
}
$r = f($argc) ?? ["a", "b"];
echo $r[0], "\n", $r[1], "\n";
"#,
    );
    assert_eq!(out, "1\n2\n");
}

/// End-to-end companion for issue #587: `??` keeps the heterogeneous result
/// array-shaped in the checker while #549 provides boxed-Mixed merge storage,
/// allowing array builtins and spread to consume the selected branch.
#[test]
fn test_null_coalesce_heterogeneous_array_result_supports_array_operations() {
    let out = compile_and_run(
        r#"<?php
function maybe(int $n) {
    return $n === 1 ? [1, 2] : null;
}
$r = maybe($argc) ?? ["a", "b"];
$spread = [...$r];
echo array_sum($r), "|", in_array(2, $r), "|", count($spread);
"#,
    );
    assert_eq!(out, "3|1|2");
}

/// Regression test for issue #549 (borrowed-cell review follow-up): a
/// borrowed `?array` parameter cell flowing through `??` into a widened
/// merge shares its payload with the caller's live array, so the widening
/// conversion must copy-on-write-split instead of rewriting the caller's
/// slots in place.
#[test]
fn test_null_coalesce_borrowed_param_cell_preserves_caller_array() {
    let out = compile_and_run(
        r#"<?php
function g(?array $a): int {
    $r = $a ?? ["x", "y"];
    return count($r);
}
$src = [1, 2];
$n = g($src);
$m = g($src);
echo $n, $m, "|", $src[0], "|", $src[1], "\n";
"#,
    );
    assert_eq!(out, "22|1|2\n");
}

/// Regression test for issue #549 (borrowed-cell review follow-up): repeated
/// borrowed-cell widenings against the same live caller array must keep the
/// heap balanced — the conversion owns exactly the payload reference the
/// unbox coercion retained.
#[test]
fn test_null_coalesce_borrowed_param_cell_loop_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function g(?array $a): int {
    $r = $a ?? ["x", "y"];
    return count($r);
}
$c = 0;
$src = [1, 2];
for ($i = 0; $i < 20; $i++) {
    if ($i % 2 == 0) {
        $c = $c + g($src);
    } else {
        $c = $c + g(null);
    }
}
echo $c, "|", $src[0];
"#,
    );
    assert_eq!(out.stdout, "40|1");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean heap summary, got: {}",
        out.stderr
    );
}

/// Regression test for issue #549 (borrowed-cell review follow-up): the
/// associative sibling of the borrowed `?array` parameter merge must also
/// preserve the caller's live hash across the widening conversion.
#[test]
fn test_null_coalesce_borrowed_assoc_param_cell_preserves_caller_hash() {
    let out = compile_and_run(
        r#"<?php
function g(?array $a): int {
    $r = $a ?? ["k" => "x"];
    return count($r);
}
$src = ["k" => 1, "n" => 2];
$n = g($src);
$m = g($src);
echo $n, $m, "|", $src["k"], "|", $src["n"], "\n";
"#,
    );
    assert_eq!(out, "22|1|2\n");
}

/// Regression test for issue #549 (borrowed-cell review follow-up): a local
/// `?array` cell read by two `??` merges must survive the first widening
/// conversion with payload and cell intact.
#[test]
fn test_null_coalesce_local_nullable_cell_survives_two_merges() {
    let out = compile_and_run(
        r#"<?php
function f(int $n): ?array {
    if ($n === 1) {
        return [1, 2];
    }
    return null;
}
$x = f($argc);
$r = $x ?? ["a", "b"];
$s = $x ?? ["c", "d"];
echo $r[0], $r[1], $s[0], $s[1], "\n";
"#,
    );
    assert_eq!(out, "1212\n");
}

/// Regression test for issue #554: `??` must select its default when a missing
/// indexed-array read has a statically-known `Str` result, while a present empty
/// string keeps `''`. Indices are bound to `$argc`-derived locals so the element
/// read stays statically `Str` (an inline arithmetic index like `$a[$argc + 6]`
/// is typed `Mixed` and would route through the boxed-Mixed null path instead),
/// forcing the miss through the `IsNull`(`Str`) branch decision: a missed string
/// slot carries the null-string pointer sentinel, a real empty string does not.
/// Covers both the first-index miss (missed outer container) and the second-index
/// miss (missed inner string slot).
#[test]
fn test_null_coalesce_string_array_miss_selects_default() {
    let out = compile_and_run(
        r#"<?php
$a = [['', 'word0'], ['', 'word1']];
$miss_i = $argc + 6;
$hit_i = $argc;
$miss_j = $argc + 6;
$hit_j = $argc - 1;
echo '[' . ($a[$miss_i][$hit_j] ?? 'dflt') . "]\n";
echo '[' . ($a[$hit_i][$miss_j] ?? 'dflt') . "]\n";
echo '[' . ($a[$hit_i][$hit_j] ?? 'dflt') . "]\n";
"#,
    );
    assert_eq!(out, "[dflt]\n[dflt]\n[]\n");
}

/// Regression test for issue #554 using the issue's verbatim literal-index
/// reproduction: `$a[7][1] ?? 'dflt'` and `$a[1][7] ?? 'dflt'` select the
/// default, and the present empty string `$a[1][0]` stays `''`, matching PHP's
/// `[dflt]` / `[dflt]` / `[]` output exactly.
#[test]
fn test_null_coalesce_string_array_miss_literal_repro() {
    let out = compile_and_run(
        r#"<?php
$a = [['', 'word0'], ['', 'word1']];
echo '[' . ($a[7][1] ?? 'dflt') . "]\n";
echo '[' . ($a[1][7] ?? 'dflt') . "]\n";
echo '[' . ($a[1][0] ?? 'dflt') . "]\n";
"#,
    );
    assert_eq!(out, "[dflt]\n[dflt]\n[]\n");
}

/// Regression test for issue #554 (string-keyed associative sibling): a missing
/// string-keyed read of a statically-`Str`-valued hash selects the `??` default,
/// a present key keeps its value, and a present empty string stays `''`. The
/// runtime-unknown key defeats folding so the miss travels the hash-get null
/// fallback into the `IsNull`(`Str`) branch decision.
#[test]
fn test_null_coalesce_assoc_string_miss_selects_default() {
    let out = compile_and_run(
        r#"<?php
$m = ['a' => 'x', 'b' => 'y'];
$miss = $argc > 100 ? 'a' : 'zzz';
echo '[' . ($m[$miss] ?? 'dflt') . "]\n";
$hit = $argc > 100 ? 'zzz' : 'a';
echo '[' . ($m[$hit] ?? 'dflt') . "]\n";
$e = ['k' => ''];
echo '[' . ($e['k'] ?? 'dflt') . "]\n";
"#,
    );
    assert_eq!(out, "[dflt]\n[x]\n[]\n");
}

/// Regression test for issue #554 (nullable-string local/return sibling): a
/// `?string` return that is null at runtime selects the `??` default, a non-null
/// return keeps its value, and a present empty-string local stays `''`. Confirms
/// the same null-string representation used for array-element misses drives the
/// coalesce decision for scalar nullable-string storage too.
#[test]
fn test_null_coalesce_nullable_string_local_selects_default() {
    let out = compile_and_run(
        r#"<?php
function f(int $n): ?string { return $n > 100 ? 'hit' : null; }
$s = f($argc);
echo '[' . ($s ?? 'dflt') . "]\n";
$t = f($argc + 1000);
echo '[' . ($t ?? 'dflt') . "]\n";
$u = '';
echo '[' . ($u ?? 'dflt') . "]\n";
"#,
    );
    assert_eq!(out, "[dflt]\n[hit]\n[]\n");
}

/// Regression test for issue #554: the `??` string-miss decision keeps the heap
/// balanced across a loop that mixes a missing read (default 'dflt') and a
/// present empty-string control (kept `''`). The missed read materializes a
/// non-owned null-string sentinel and the default is a persistent literal, so no
/// per-iteration allocation should leak.
#[test]
fn test_null_coalesce_string_array_miss_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$a = [['', 'word0'], ['', 'word1']];
$hit_i = $argc;
$miss_j = $argc + 6;
$hit_j = $argc - 1;
$c = 0;
for ($i = 0; $i < 20; $i++) {
    $miss = ($a[$hit_i][$miss_j] ?? 'dflt');
    $present = ($a[$hit_i][$hit_j] ?? 'dflt');
    $c = $c + strlen($miss) + strlen($present);
}
echo $c;
"#,
    );
    assert_eq!(out.stdout, "80");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean heap summary, got: {}",
        out.stderr
    );
}

/// Regression test for issue #554: the `??` string-miss path must stay silent,
/// matching PHP. A bare missed read warns "Undefined array key", but wrapping it
/// in `??` suppresses the warning (issue #533's warning behavior on the coalesce
/// path). Asserts the default is selected with no diagnostics on stderr.
#[test]
fn test_null_coalesce_string_array_miss_is_silent() {
    let out = compile_and_run_capture(
        r#"<?php
$a = [['', 'word0'], ['', 'word1']];
$hit_i = $argc;
$miss_j = $argc + 6;
echo ($a[$hit_i][$miss_j] ?? 'dflt');
"#,
    );
    assert_eq!(out.stdout, "dflt");
    assert!(
        !out.stderr.contains("Undefined") && !out.stderr.contains("Warning"),
        "expected silent coalesce path, got stderr: {}",
        out.stderr
    );
    assert!(out.success, "program should exit successfully");
}
