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
