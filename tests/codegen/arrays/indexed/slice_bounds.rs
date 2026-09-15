//! Purpose:
//! Regression tests for PHP's `array_slice()`/`array_splice()` offset and length window
//! arithmetic, covering the negative-`$length` semantics that the runtime helpers used to
//! encode with an ambiguous `-1` "until the end" sentinel.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Every expected string in this file is verbatim `LC_ALL=C php` 8.4 output for the same fixture.
//! - The matrices assert `count()` on every result, so a runtime helper that publishes a negative or
//!   over-large logical length in the array header can never pass again.
//! - Fixtures drive the offsets/lengths through `foreach` over literal int arrays instead of a helper
//!   function, because passing a freshly sliced array straight into a user function hits an unrelated
//!   pre-existing ownership gap that would mask the behavior under test.
//! - The scalar fixtures exercise `__rt_array_slice`/`__rt_array_splice`, the `[[1], ...]` fixtures
//!   exercise the refcounted variants, and the `$m["arr"]` and untyped-parameter fixtures exercise
//!   the boxed-`Mixed` path.
//! - The untyped-parameter fixtures also pin the EIR result LAYOUT of a boxed-`Mixed` slice: the
//!   checker specializes `function top($scores)` from its call site, EIR gives every undeclared
//!   parameter the boxed-`Mixed` ABI contract, and the slice result must follow the operands rather
//!   than the checker's narrower call-site type.
//! - The `PHP_INT_MAX`/`PHP_INT_MIN` fixture derives its bounds from `$argc` so the frontend cannot
//!   fold the extreme offsets and lengths away before they reach the runtime helpers.

use super::*;

/// Regression: `array_slice()` with a negative `$length` must stop that many elements before the
/// end of the array and clamp to an empty result, never report a negative `count()`.
///
/// `array_slice([1,2,3,4], 0, -10)` used to return an array whose header claimed `-10` elements and
/// `array_slice([1,2,3,4], 2, -1)` used to return 2 elements because `-1` doubled as the runtime
/// "slice to the end" sentinel.
#[test]
fn test_array_slice_negative_length_clamps_instead_of_reporting_negative_count() {
    let out = compile_and_run(
        r#"<?php
$a = [1, 2, 3, 4];
echo count(array_slice($a, 0, -10)), "\n";
echo count(array_slice($a, 0, -4)), "\n";
echo count(array_slice($a, 2, -1)), "\n";
echo count(array_slice($a, -10, -3)), "\n";
$b = [1, 2, 3, 4];
$removed = array_splice($b, 0, -10);
echo count($removed), " ", count($b), "\n";
$c = [1, 2, 3, 4];
$removed = array_splice($c, 2, -1);
echo count($removed), " ", count($c), "\n";
$d = [1, 2, 3, 4];
$removed = array_splice($d, -10, -10);
echo count($removed), " ", count($d), "\n";
"#,
    );
    assert_eq!(
        out,
        r#"0
0
1
1
0 4
1 3
0 4
"#
    );
}

/// Regression: the full `array_slice()` `$offset` x `$length` matrix must match PHP.
///
/// Covers negative/zero/positive/past-the-end offsets crossed with omitted, `null`, negative, zero
/// and positive lengths on a four-element indexed array, asserting both `count()` and the
/// reindexed key/value pairs of every result.
#[test]
fn test_array_slice_offset_length_matrix_matches_php() {
    let out = compile_and_run(
        r#"<?php
$offs = [-10, -4, -3, -1, 0, 1, 3, 4, 10];
$lens = [-10, -4, -3, -1, 0, 1, 3, 4, 10];
foreach ($offs as $o) {
    $r = array_slice([1, 2, 3, 4], $o);
    echo "off=", $o, " len=omit cnt=", count($r);
    foreach ($r as $k => $v) { echo " ", $k, "=", $v; }
    echo "\n";
    $r = array_slice([1, 2, 3, 4], $o, null);
    echo "off=", $o, " len=null cnt=", count($r);
    foreach ($r as $k => $v) { echo " ", $k, "=", $v; }
    echo "\n";
    foreach ($lens as $l) {
        $r = array_slice([1, 2, 3, 4], $o, $l);
        echo "off=", $o, " len=", $l, " cnt=", count($r);
        foreach ($r as $k => $v) { echo " ", $k, "=", $v; }
        echo "\n";
    }
}
"#,
    );
    assert_eq!(
        out,
        r#"off=-10 len=omit cnt=4 0=1 1=2 2=3 3=4
off=-10 len=null cnt=4 0=1 1=2 2=3 3=4
off=-10 len=-10 cnt=0
off=-10 len=-4 cnt=0
off=-10 len=-3 cnt=1 0=1
off=-10 len=-1 cnt=3 0=1 1=2 2=3
off=-10 len=0 cnt=0
off=-10 len=1 cnt=1 0=1
off=-10 len=3 cnt=3 0=1 1=2 2=3
off=-10 len=4 cnt=4 0=1 1=2 2=3 3=4
off=-10 len=10 cnt=4 0=1 1=2 2=3 3=4
off=-4 len=omit cnt=4 0=1 1=2 2=3 3=4
off=-4 len=null cnt=4 0=1 1=2 2=3 3=4
off=-4 len=-10 cnt=0
off=-4 len=-4 cnt=0
off=-4 len=-3 cnt=1 0=1
off=-4 len=-1 cnt=3 0=1 1=2 2=3
off=-4 len=0 cnt=0
off=-4 len=1 cnt=1 0=1
off=-4 len=3 cnt=3 0=1 1=2 2=3
off=-4 len=4 cnt=4 0=1 1=2 2=3 3=4
off=-4 len=10 cnt=4 0=1 1=2 2=3 3=4
off=-3 len=omit cnt=3 0=2 1=3 2=4
off=-3 len=null cnt=3 0=2 1=3 2=4
off=-3 len=-10 cnt=0
off=-3 len=-4 cnt=0
off=-3 len=-3 cnt=0
off=-3 len=-1 cnt=2 0=2 1=3
off=-3 len=0 cnt=0
off=-3 len=1 cnt=1 0=2
off=-3 len=3 cnt=3 0=2 1=3 2=4
off=-3 len=4 cnt=3 0=2 1=3 2=4
off=-3 len=10 cnt=3 0=2 1=3 2=4
off=-1 len=omit cnt=1 0=4
off=-1 len=null cnt=1 0=4
off=-1 len=-10 cnt=0
off=-1 len=-4 cnt=0
off=-1 len=-3 cnt=0
off=-1 len=-1 cnt=0
off=-1 len=0 cnt=0
off=-1 len=1 cnt=1 0=4
off=-1 len=3 cnt=1 0=4
off=-1 len=4 cnt=1 0=4
off=-1 len=10 cnt=1 0=4
off=0 len=omit cnt=4 0=1 1=2 2=3 3=4
off=0 len=null cnt=4 0=1 1=2 2=3 3=4
off=0 len=-10 cnt=0
off=0 len=-4 cnt=0
off=0 len=-3 cnt=1 0=1
off=0 len=-1 cnt=3 0=1 1=2 2=3
off=0 len=0 cnt=0
off=0 len=1 cnt=1 0=1
off=0 len=3 cnt=3 0=1 1=2 2=3
off=0 len=4 cnt=4 0=1 1=2 2=3 3=4
off=0 len=10 cnt=4 0=1 1=2 2=3 3=4
off=1 len=omit cnt=3 0=2 1=3 2=4
off=1 len=null cnt=3 0=2 1=3 2=4
off=1 len=-10 cnt=0
off=1 len=-4 cnt=0
off=1 len=-3 cnt=0
off=1 len=-1 cnt=2 0=2 1=3
off=1 len=0 cnt=0
off=1 len=1 cnt=1 0=2
off=1 len=3 cnt=3 0=2 1=3 2=4
off=1 len=4 cnt=3 0=2 1=3 2=4
off=1 len=10 cnt=3 0=2 1=3 2=4
off=3 len=omit cnt=1 0=4
off=3 len=null cnt=1 0=4
off=3 len=-10 cnt=0
off=3 len=-4 cnt=0
off=3 len=-3 cnt=0
off=3 len=-1 cnt=0
off=3 len=0 cnt=0
off=3 len=1 cnt=1 0=4
off=3 len=3 cnt=1 0=4
off=3 len=4 cnt=1 0=4
off=3 len=10 cnt=1 0=4
off=4 len=omit cnt=0
off=4 len=null cnt=0
off=4 len=-10 cnt=0
off=4 len=-4 cnt=0
off=4 len=-3 cnt=0
off=4 len=-1 cnt=0
off=4 len=0 cnt=0
off=4 len=1 cnt=0
off=4 len=3 cnt=0
off=4 len=4 cnt=0
off=4 len=10 cnt=0
off=10 len=omit cnt=0
off=10 len=null cnt=0
off=10 len=-10 cnt=0
off=10 len=-4 cnt=0
off=10 len=-3 cnt=0
off=10 len=-1 cnt=0
off=10 len=0 cnt=0
off=10 len=1 cnt=0
off=10 len=3 cnt=0
off=10 len=4 cnt=0
off=10 len=10 cnt=0
"#
    );
}

/// Regression: the full `array_splice()` `$offset` x `$length` matrix must match PHP.
///
/// Asserts both the removed-elements array and what stays in the spliced source array. A negative
/// `$length` used to make the ARM64 helper walk its compaction cursor backwards and publish a source
/// length longer than the allocation, so the source contents are checked on every row.
#[test]
fn test_array_splice_offset_length_matrix_matches_php() {
    let out = compile_and_run(
        r#"<?php
$offs = [-10, -4, -3, -1, 0, 1, 3, 4, 10];
$lens = [-10, -4, -3, -1, 0, 1, 3, 4, 10];
foreach ($offs as $o) {
    $a = [1, 2, 3, 4];
    $r = array_splice($a, $o);
    echo "off=", $o, " len=omit r=", count($r);
    foreach ($r as $k => $v) { echo " ", $k, "=", $v; }
    echo " a=", count($a);
    foreach ($a as $k => $v) { echo " ", $k, "=", $v; }
    echo "\n";
    $a = [1, 2, 3, 4];
    $r = array_splice($a, $o, null);
    echo "off=", $o, " len=null r=", count($r);
    foreach ($r as $k => $v) { echo " ", $k, "=", $v; }
    echo " a=", count($a);
    foreach ($a as $k => $v) { echo " ", $k, "=", $v; }
    echo "\n";
    foreach ($lens as $l) {
        $a = [1, 2, 3, 4];
        $r = array_splice($a, $o, $l);
        echo "off=", $o, " len=", $l, " r=", count($r);
        foreach ($r as $k => $v) { echo " ", $k, "=", $v; }
        echo " a=", count($a);
        foreach ($a as $k => $v) { echo " ", $k, "=", $v; }
        echo "\n";
    }
}
"#,
    );
    assert_eq!(
        out,
        r#"off=-10 len=omit r=4 0=1 1=2 2=3 3=4 a=0
off=-10 len=null r=4 0=1 1=2 2=3 3=4 a=0
off=-10 len=-10 r=0 a=4 0=1 1=2 2=3 3=4
off=-10 len=-4 r=0 a=4 0=1 1=2 2=3 3=4
off=-10 len=-3 r=1 0=1 a=3 0=2 1=3 2=4
off=-10 len=-1 r=3 0=1 1=2 2=3 a=1 0=4
off=-10 len=0 r=0 a=4 0=1 1=2 2=3 3=4
off=-10 len=1 r=1 0=1 a=3 0=2 1=3 2=4
off=-10 len=3 r=3 0=1 1=2 2=3 a=1 0=4
off=-10 len=4 r=4 0=1 1=2 2=3 3=4 a=0
off=-10 len=10 r=4 0=1 1=2 2=3 3=4 a=0
off=-4 len=omit r=4 0=1 1=2 2=3 3=4 a=0
off=-4 len=null r=4 0=1 1=2 2=3 3=4 a=0
off=-4 len=-10 r=0 a=4 0=1 1=2 2=3 3=4
off=-4 len=-4 r=0 a=4 0=1 1=2 2=3 3=4
off=-4 len=-3 r=1 0=1 a=3 0=2 1=3 2=4
off=-4 len=-1 r=3 0=1 1=2 2=3 a=1 0=4
off=-4 len=0 r=0 a=4 0=1 1=2 2=3 3=4
off=-4 len=1 r=1 0=1 a=3 0=2 1=3 2=4
off=-4 len=3 r=3 0=1 1=2 2=3 a=1 0=4
off=-4 len=4 r=4 0=1 1=2 2=3 3=4 a=0
off=-4 len=10 r=4 0=1 1=2 2=3 3=4 a=0
off=-3 len=omit r=3 0=2 1=3 2=4 a=1 0=1
off=-3 len=null r=3 0=2 1=3 2=4 a=1 0=1
off=-3 len=-10 r=0 a=4 0=1 1=2 2=3 3=4
off=-3 len=-4 r=0 a=4 0=1 1=2 2=3 3=4
off=-3 len=-3 r=0 a=4 0=1 1=2 2=3 3=4
off=-3 len=-1 r=2 0=2 1=3 a=2 0=1 1=4
off=-3 len=0 r=0 a=4 0=1 1=2 2=3 3=4
off=-3 len=1 r=1 0=2 a=3 0=1 1=3 2=4
off=-3 len=3 r=3 0=2 1=3 2=4 a=1 0=1
off=-3 len=4 r=3 0=2 1=3 2=4 a=1 0=1
off=-3 len=10 r=3 0=2 1=3 2=4 a=1 0=1
off=-1 len=omit r=1 0=4 a=3 0=1 1=2 2=3
off=-1 len=null r=1 0=4 a=3 0=1 1=2 2=3
off=-1 len=-10 r=0 a=4 0=1 1=2 2=3 3=4
off=-1 len=-4 r=0 a=4 0=1 1=2 2=3 3=4
off=-1 len=-3 r=0 a=4 0=1 1=2 2=3 3=4
off=-1 len=-1 r=0 a=4 0=1 1=2 2=3 3=4
off=-1 len=0 r=0 a=4 0=1 1=2 2=3 3=4
off=-1 len=1 r=1 0=4 a=3 0=1 1=2 2=3
off=-1 len=3 r=1 0=4 a=3 0=1 1=2 2=3
off=-1 len=4 r=1 0=4 a=3 0=1 1=2 2=3
off=-1 len=10 r=1 0=4 a=3 0=1 1=2 2=3
off=0 len=omit r=4 0=1 1=2 2=3 3=4 a=0
off=0 len=null r=4 0=1 1=2 2=3 3=4 a=0
off=0 len=-10 r=0 a=4 0=1 1=2 2=3 3=4
off=0 len=-4 r=0 a=4 0=1 1=2 2=3 3=4
off=0 len=-3 r=1 0=1 a=3 0=2 1=3 2=4
off=0 len=-1 r=3 0=1 1=2 2=3 a=1 0=4
off=0 len=0 r=0 a=4 0=1 1=2 2=3 3=4
off=0 len=1 r=1 0=1 a=3 0=2 1=3 2=4
off=0 len=3 r=3 0=1 1=2 2=3 a=1 0=4
off=0 len=4 r=4 0=1 1=2 2=3 3=4 a=0
off=0 len=10 r=4 0=1 1=2 2=3 3=4 a=0
off=1 len=omit r=3 0=2 1=3 2=4 a=1 0=1
off=1 len=null r=3 0=2 1=3 2=4 a=1 0=1
off=1 len=-10 r=0 a=4 0=1 1=2 2=3 3=4
off=1 len=-4 r=0 a=4 0=1 1=2 2=3 3=4
off=1 len=-3 r=0 a=4 0=1 1=2 2=3 3=4
off=1 len=-1 r=2 0=2 1=3 a=2 0=1 1=4
off=1 len=0 r=0 a=4 0=1 1=2 2=3 3=4
off=1 len=1 r=1 0=2 a=3 0=1 1=3 2=4
off=1 len=3 r=3 0=2 1=3 2=4 a=1 0=1
off=1 len=4 r=3 0=2 1=3 2=4 a=1 0=1
off=1 len=10 r=3 0=2 1=3 2=4 a=1 0=1
off=3 len=omit r=1 0=4 a=3 0=1 1=2 2=3
off=3 len=null r=1 0=4 a=3 0=1 1=2 2=3
off=3 len=-10 r=0 a=4 0=1 1=2 2=3 3=4
off=3 len=-4 r=0 a=4 0=1 1=2 2=3 3=4
off=3 len=-3 r=0 a=4 0=1 1=2 2=3 3=4
off=3 len=-1 r=0 a=4 0=1 1=2 2=3 3=4
off=3 len=0 r=0 a=4 0=1 1=2 2=3 3=4
off=3 len=1 r=1 0=4 a=3 0=1 1=2 2=3
off=3 len=3 r=1 0=4 a=3 0=1 1=2 2=3
off=3 len=4 r=1 0=4 a=3 0=1 1=2 2=3
off=3 len=10 r=1 0=4 a=3 0=1 1=2 2=3
off=4 len=omit r=0 a=4 0=1 1=2 2=3 3=4
off=4 len=null r=0 a=4 0=1 1=2 2=3 3=4
off=4 len=-10 r=0 a=4 0=1 1=2 2=3 3=4
off=4 len=-4 r=0 a=4 0=1 1=2 2=3 3=4
off=4 len=-3 r=0 a=4 0=1 1=2 2=3 3=4
off=4 len=-1 r=0 a=4 0=1 1=2 2=3 3=4
off=4 len=0 r=0 a=4 0=1 1=2 2=3 3=4
off=4 len=1 r=0 a=4 0=1 1=2 2=3 3=4
off=4 len=3 r=0 a=4 0=1 1=2 2=3 3=4
off=4 len=4 r=0 a=4 0=1 1=2 2=3 3=4
off=4 len=10 r=0 a=4 0=1 1=2 2=3 3=4
off=10 len=omit r=0 a=4 0=1 1=2 2=3 3=4
off=10 len=null r=0 a=4 0=1 1=2 2=3 3=4
off=10 len=-10 r=0 a=4 0=1 1=2 2=3 3=4
off=10 len=-4 r=0 a=4 0=1 1=2 2=3 3=4
off=10 len=-3 r=0 a=4 0=1 1=2 2=3 3=4
off=10 len=-1 r=0 a=4 0=1 1=2 2=3 3=4
off=10 len=0 r=0 a=4 0=1 1=2 2=3 3=4
off=10 len=1 r=0 a=4 0=1 1=2 2=3 3=4
off=10 len=3 r=0 a=4 0=1 1=2 2=3 3=4
off=10 len=4 r=0 a=4 0=1 1=2 2=3 3=4
off=10 len=10 r=0 a=4 0=1 1=2 2=3 3=4
"#
    );
}

/// Regression: the refcounted slice/splice helpers apply the same window arithmetic.
///
/// An `array<array<int>>` source routes through `__rt_array_slice_refcounted` and
/// `__rt_array_splice_refcounted`, which retain each copied payload, so the negative-length clamp has
/// to hold there too or the retain loop runs off the end of the source payload.
#[test]
fn test_slice_splice_refcounted_offset_length_matrix_matches_php() {
    let out = compile_and_run(
        r#"<?php
$lens = [-10, -4, -3, -1, 0, 1, 3, 4, 10];
foreach ([-10, -3, 0, 2, 4, 10] as $o) {
    $r = array_slice([[1], [2], [3], [4]], $o);
    echo "slice off=", $o, " len=omit cnt=", count($r);
    foreach ($r as $v) { echo " ", $v[0]; }
    echo "\n";
    foreach ($lens as $l) {
        $r = array_slice([[1], [2], [3], [4]], $o, $l);
        echo "slice off=", $o, " len=", $l, " cnt=", count($r);
        foreach ($r as $v) { echo " ", $v[0]; }
        echo "\n";
    }
    $a = [[1], [2], [3], [4]];
    $r = array_splice($a, $o);
    echo "splice off=", $o, " len=omit r=", count($r);
    foreach ($r as $v) { echo " ", $v[0]; }
    echo " a=", count($a);
    foreach ($a as $v) { echo " ", $v[0]; }
    echo "\n";
    foreach ($lens as $l) {
        $a = [[1], [2], [3], [4]];
        $r = array_splice($a, $o, $l);
        echo "splice off=", $o, " len=", $l, " r=", count($r);
        foreach ($r as $v) { echo " ", $v[0]; }
        echo " a=", count($a);
        foreach ($a as $v) { echo " ", $v[0]; }
        echo "\n";
    }
}
"#,
    );
    assert_eq!(
        out,
        r#"slice off=-10 len=omit cnt=4 1 2 3 4
slice off=-10 len=-10 cnt=0
slice off=-10 len=-4 cnt=0
slice off=-10 len=-3 cnt=1 1
slice off=-10 len=-1 cnt=3 1 2 3
slice off=-10 len=0 cnt=0
slice off=-10 len=1 cnt=1 1
slice off=-10 len=3 cnt=3 1 2 3
slice off=-10 len=4 cnt=4 1 2 3 4
slice off=-10 len=10 cnt=4 1 2 3 4
splice off=-10 len=omit r=4 1 2 3 4 a=0
splice off=-10 len=-10 r=0 a=4 1 2 3 4
splice off=-10 len=-4 r=0 a=4 1 2 3 4
splice off=-10 len=-3 r=1 1 a=3 2 3 4
splice off=-10 len=-1 r=3 1 2 3 a=1 4
splice off=-10 len=0 r=0 a=4 1 2 3 4
splice off=-10 len=1 r=1 1 a=3 2 3 4
splice off=-10 len=3 r=3 1 2 3 a=1 4
splice off=-10 len=4 r=4 1 2 3 4 a=0
splice off=-10 len=10 r=4 1 2 3 4 a=0
slice off=-3 len=omit cnt=3 2 3 4
slice off=-3 len=-10 cnt=0
slice off=-3 len=-4 cnt=0
slice off=-3 len=-3 cnt=0
slice off=-3 len=-1 cnt=2 2 3
slice off=-3 len=0 cnt=0
slice off=-3 len=1 cnt=1 2
slice off=-3 len=3 cnt=3 2 3 4
slice off=-3 len=4 cnt=3 2 3 4
slice off=-3 len=10 cnt=3 2 3 4
splice off=-3 len=omit r=3 2 3 4 a=1 1
splice off=-3 len=-10 r=0 a=4 1 2 3 4
splice off=-3 len=-4 r=0 a=4 1 2 3 4
splice off=-3 len=-3 r=0 a=4 1 2 3 4
splice off=-3 len=-1 r=2 2 3 a=2 1 4
splice off=-3 len=0 r=0 a=4 1 2 3 4
splice off=-3 len=1 r=1 2 a=3 1 3 4
splice off=-3 len=3 r=3 2 3 4 a=1 1
splice off=-3 len=4 r=3 2 3 4 a=1 1
splice off=-3 len=10 r=3 2 3 4 a=1 1
slice off=0 len=omit cnt=4 1 2 3 4
slice off=0 len=-10 cnt=0
slice off=0 len=-4 cnt=0
slice off=0 len=-3 cnt=1 1
slice off=0 len=-1 cnt=3 1 2 3
slice off=0 len=0 cnt=0
slice off=0 len=1 cnt=1 1
slice off=0 len=3 cnt=3 1 2 3
slice off=0 len=4 cnt=4 1 2 3 4
slice off=0 len=10 cnt=4 1 2 3 4
splice off=0 len=omit r=4 1 2 3 4 a=0
splice off=0 len=-10 r=0 a=4 1 2 3 4
splice off=0 len=-4 r=0 a=4 1 2 3 4
splice off=0 len=-3 r=1 1 a=3 2 3 4
splice off=0 len=-1 r=3 1 2 3 a=1 4
splice off=0 len=0 r=0 a=4 1 2 3 4
splice off=0 len=1 r=1 1 a=3 2 3 4
splice off=0 len=3 r=3 1 2 3 a=1 4
splice off=0 len=4 r=4 1 2 3 4 a=0
splice off=0 len=10 r=4 1 2 3 4 a=0
slice off=2 len=omit cnt=2 3 4
slice off=2 len=-10 cnt=0
slice off=2 len=-4 cnt=0
slice off=2 len=-3 cnt=0
slice off=2 len=-1 cnt=1 3
slice off=2 len=0 cnt=0
slice off=2 len=1 cnt=1 3
slice off=2 len=3 cnt=2 3 4
slice off=2 len=4 cnt=2 3 4
slice off=2 len=10 cnt=2 3 4
splice off=2 len=omit r=2 3 4 a=2 1 2
splice off=2 len=-10 r=0 a=4 1 2 3 4
splice off=2 len=-4 r=0 a=4 1 2 3 4
splice off=2 len=-3 r=0 a=4 1 2 3 4
splice off=2 len=-1 r=1 3 a=3 1 2 4
splice off=2 len=0 r=0 a=4 1 2 3 4
splice off=2 len=1 r=1 3 a=3 1 2 4
splice off=2 len=3 r=2 3 4 a=2 1 2
splice off=2 len=4 r=2 3 4 a=2 1 2
splice off=2 len=10 r=2 3 4 a=2 1 2
slice off=4 len=omit cnt=0
slice off=4 len=-10 cnt=0
slice off=4 len=-4 cnt=0
slice off=4 len=-3 cnt=0
slice off=4 len=-1 cnt=0
slice off=4 len=0 cnt=0
slice off=4 len=1 cnt=0
slice off=4 len=3 cnt=0
slice off=4 len=4 cnt=0
slice off=4 len=10 cnt=0
splice off=4 len=omit r=0 a=4 1 2 3 4
splice off=4 len=-10 r=0 a=4 1 2 3 4
splice off=4 len=-4 r=0 a=4 1 2 3 4
splice off=4 len=-3 r=0 a=4 1 2 3 4
splice off=4 len=-1 r=0 a=4 1 2 3 4
splice off=4 len=0 r=0 a=4 1 2 3 4
splice off=4 len=1 r=0 a=4 1 2 3 4
splice off=4 len=3 r=0 a=4 1 2 3 4
splice off=4 len=4 r=0 a=4 1 2 3 4
splice off=4 len=10 r=0 a=4 1 2 3 4
slice off=10 len=omit cnt=0
slice off=10 len=-10 cnt=0
slice off=10 len=-4 cnt=0
slice off=10 len=-3 cnt=0
slice off=10 len=-1 cnt=0
slice off=10 len=0 cnt=0
slice off=10 len=1 cnt=0
slice off=10 len=3 cnt=0
slice off=10 len=4 cnt=0
slice off=10 len=10 cnt=0
splice off=10 len=omit r=0 a=4 1 2 3 4
splice off=10 len=-10 r=0 a=4 1 2 3 4
splice off=10 len=-4 r=0 a=4 1 2 3 4
splice off=10 len=-3 r=0 a=4 1 2 3 4
splice off=10 len=-1 r=0 a=4 1 2 3 4
splice off=10 len=0 r=0 a=4 1 2 3 4
splice off=10 len=1 r=0 a=4 1 2 3 4
splice off=10 len=3 r=0 a=4 1 2 3 4
splice off=10 len=4 r=0 a=4 1 2 3 4
splice off=10 len=10 r=0 a=4 1 2 3 4
"#
    );
}

/// Regression: boxed-`Mixed` operands and a `$length` that is only known to be `null` at runtime.
///
/// `$m["arr"]` is a heterogeneous-hash payload, so the call routes through the boxed-`Mixed` slice
/// lowering; `$m["z"]` and `$opts[$argc - 1]` are `null` values whose nullness is not visible to the
/// type checker, so the length-present flag has to be derived from the runtime `Mixed` tag.
#[test]
fn test_array_slice_boxed_mixed_and_runtime_null_length_match_php() {
    let out = compile_and_run(
        r#"<?php
$m = ["arr" => [1, 2, 3, 4], "n" => 2, "z" => null];
$a = array_slice($m["arr"], 0, -10);
echo "a=", count($a), "\n";
$b = array_slice($m["arr"], 2, -1);
echo "b=", count($b);
foreach ($b as $v) { echo " ", $v; }
echo "\n";
$c = array_slice($m["arr"], -3, $m["n"]);
echo "c=", count($c);
foreach ($c as $v) { echo " ", $v; }
echo "\n";
$d = array_slice($m["arr"], 1, $m["z"]);
echo "d=", count($d);
foreach ($d as $v) { echo " ", $v; }
echo "\n";
$opts = [null, -1, 2];
$e = array_slice([1, 2, 3, 4], 0, $opts[$argc - 1]);
echo "e=", count($e);
foreach ($e as $v) { echo " ", $v; }
echo "\n";
$f = array_slice([1, 2, 3, 4], 0, $opts[$argc]);
echo "f=", count($f);
foreach ($f as $v) { echo " ", $v; }
echo "\n";
"#,
    );
    assert_eq!(
        out,
        r#"a=0
b=1 3
c=2 2 3
d=3 2 3 4
e=4 1 2 3 4
f=3 1 2 3
"#
    );
}

/// Regression: slicing an array of associative arrays received through an untyped parameter.
///
/// The checker specializes `top($scores)` to `array<array<string, string>>` from its only call site,
/// but EIR gives every undeclared parameter the boxed-`Mixed` ABI contract, so the slice helper
/// really produces an array of boxed cells. Taking the checker's narrower call-site type as the EIR
/// result layout made the backend reject the call outright ("array_slice result element PHP type
/// AssocArray { key: Str, value: Str } for source element PHP type Mixed"); reading each element as
/// a raw hash pointer instead would have been the silent version of the same bug.
#[test]
fn test_array_slice_of_assoc_rows_through_untyped_parameter_matches_php() {
    let out = compile_and_run(
        r#"<?php
function top($scores) {
    $b = array_slice($scores, 0, 1);
    echo count($b), ":", $b[0]["name"], "\n";
    $c = array_slice($scores, 1);
    echo count($c), ":", $c[0]["name"], "\n";
    foreach (array_slice($scores, -2, 2) as $k => $row) {
        echo $k, "=", $row["name"], "|";
    }
    echo "\n";
}
top([["name" => "Ada"], ["name" => "Bob"], ["name" => "Cy"]]);
"#,
    );
    assert_eq!(
        out,
        r#"1:Ada
2:Bob
0=Bob|1=Cy|
"#
    );
}

/// Regression: the same boxed-`Mixed` receiver with scalar payloads, including negative lengths.
///
/// `int` and `string` element types hit the same checker-versus-EIR disagreement as the associative
/// rows above, and the negative-`$length` rows keep the shared window arithmetic covered on the
/// boxed-`Mixed` lowering rather than only on the typed helpers.
#[test]
fn test_array_slice_of_scalars_through_untyped_parameter_matches_php() {
    let out = compile_and_run(
        r#"<?php
function ints($values) {
    $b = array_slice($values, 1, 2);
    echo count($b), ":", $b[0], ",", $b[1], "\n";
    echo count(array_slice($values, 0, -3)), "\n";
    echo count(array_slice($values, 2, -1)), "\n";
}
ints([10, 20, 30, 40]);
function names($values) {
    echo implode(",", array_slice($values, 1)), "\n";
    echo implode(",", array_slice($values, -2, 1)), "\n";
}
names(["a", "b", "c"]);
"#,
    );
    assert_eq!(
        out,
        r#"2:20,30
1
1
b,c
b
"#
    );
}

/// Regression: slicing a slice, where the intermediate result is a widened boxed-`Mixed` array.
///
/// A boxed-`Mixed` slice widens its elements to `Mixed` in EIR while the checker keeps typing the
/// receiving variable with the precise element type, so the SECOND slice hits the same
/// checker-versus-EIR disagreement as an untyped parameter does, one step removed from the
/// parameter itself.
#[test]
fn test_chained_array_slice_through_untyped_parameter_matches_php() {
    let out = compile_and_run(
        r#"<?php
function chained($values) {
    $b = array_slice($values, 0, 3);
    $c = array_slice($b, 1, 1);
    echo count($c), ":", $c[0], "\n";
    $d = array_slice($b, -2);
    echo implode(",", $d), "\n";
}
chained([1, 2, 3, 4]);
function chained_strings($values) {
    $b = array_slice($values, 0, 3);
    echo implode(",", array_slice($b, 1, 2)), "\n";
}
chained_strings(["a", "b", "c", "d"]);
"#,
    );
    assert_eq!(
        out,
        r#"1:2
2,3
b,c
"#
    );
}

/// Regression: `PHP_INT_MAX`/`PHP_INT_MIN` offsets and lengths must clamp, not wrap.
///
/// The normalization adds `count + $offset` and `available + $length`; both additions mix a
/// non-negative operand with a negative one, so neither can overflow a signed 64-bit word. The
/// bounds come from `$argc` so the frontend cannot fold them away before codegen.
#[test]
fn test_slice_splice_extreme_offsets_and_lengths_clamp_without_overflow() {
    let out = compile_and_run(
        r#"<?php
$n = $argc - 1;
$max = PHP_INT_MAX - $n;
$min = PHP_INT_MIN + $n;
echo count(array_slice([1,2,3,4], 0, $max)), "\n";
echo count(array_slice([1,2,3,4], 0, $min)), "\n";
echo count(array_slice([1,2,3,4], $min, $max)), "\n";
echo count(array_slice([1,2,3,4], $max, 2)), "\n";
echo count(array_slice([1,2,3,4], $min)), "\n";
echo count(array_slice([1,2,3,4], 2, $min)), "\n";
$a = [1,2,3,4];
$r = array_splice($a, 0, $min);
echo count($r), " ", count($a), "\n";
$b = [1,2,3,4];
$r = array_splice($b, $min, $max);
echo count($r), " ", count($b), "\n";
$c = [1,2,3,4];
$r = array_splice($c, $max, $min);
echo count($r), " ", count($c), "\n";
$d = [1,2,3,4];
$r = array_splice($d, 2, $min);
echo count($r), " ", count($d), "\n";
"#,
    );
    assert_eq!(
        out,
        r#"4
0
4
0
4
0
0 4
4 0
0 4
0 4
"#
    );
}


/// Regression for #675: `array_slice()` accepts an indexed `array<string>`.
///
/// An indexed string array stores 16-byte `{pointer, length}` slots. `__rt_array_slice` and
/// `__rt_array_slice_refcounted` copy 8 bytes per element, so neither can carry a pair — the
/// lowering refused a string receiver outright:
///
/// ```text
/// unsupported EIR backend feature: array_slice indexed-array element PHP type Str
/// ```
///
/// `__rt_array_slice_str` copies the pair, and the whole `$offset`/`$length` matrix runs through
/// the SAME `slice_bounds` prologue as every other variant — so these rows assert that the new
/// helper inherited the semantics rather than re-deriving them: negative offsets counting back
/// from the end, a negative `$length` stopping before the end, clamping at both ends, and an
/// omitted or `null` length slicing to the end.
///
/// The last four rows are about OWNERSHIP, which is where a 16-byte copy goes wrong quietly.
/// `array_slice()` leaves its argument alone and a string array owns its bytes exclusively, so
/// the copy duplicates through `__rt_array_push_str`. Writing into the result must not disturb
/// the source, a slice must outlive the local it came from, and a slice of a slice must be
/// independent again. The long-string row forces real heap buffers rather than anything that
/// might be inline.
///
/// Every expectation is the host PHP 8.5.10 output for the same fixture.
#[test]
fn test_array_slice_on_indexed_string_array() {
    let out = compile_and_run(
        r#"<?php
$s = ["alpha", "bravo", "charlie", "delta", "echo"];

function row(string $label, array $a): void {
    echo $label, "=[", implode(",", $a), "] n=", count($a), "\n";
}

row("mid", array_slice($s, 1, 2));
row("from", array_slice($s, 2));
row("all", array_slice($s, 0));
row("neg-off", array_slice($s, -2));
row("neg-off-len", array_slice($s, -3, 2));
row("neg-len", array_slice($s, 1, -1));
row("neg-both", array_slice($s, -4, -2));
row("zero-len", array_slice($s, 1, 0));
row("past-end", array_slice($s, 99));
row("too-long", array_slice($s, 3, 99));
row("too-far-back", array_slice($s, -99, 2));
row("empty-src", array_slice([], 0, 3));
row("null-len", array_slice($s, 1, null));

$copy = array_slice($s, 1, 2);
$copy[0] = "MUTATED";
row("after-mutate-copy", $copy);
row("source-intact", $s);

function detached(): array {
    $local = ["x", "y", "z"];
    return array_slice($local, 1);
}
row("detached", detached());
row("nested", array_slice(array_slice($s, 1, 3), 1));

$long = [str_repeat("q", 100), str_repeat("w", 200), str_repeat("e", 300)];
$cut = array_slice($long, 1, 2);
echo "long=", strlen($cut[0]), ",", strlen($cut[1]), " first=", $cut[0][0], " second=", $cut[1][0], "\n";
"#,
    );
    assert_eq!(
        out,
        concat!(
            "mid=[bravo,charlie] n=2\n",
            "from=[charlie,delta,echo] n=3\n",
            "all=[alpha,bravo,charlie,delta,echo] n=5\n",
            "neg-off=[delta,echo] n=2\n",
            "neg-off-len=[charlie,delta] n=2\n",
            "neg-len=[bravo,charlie,delta] n=3\n",
            "neg-both=[bravo,charlie] n=2\n",
            "zero-len=[] n=0\n",
            "past-end=[] n=0\n",
            "too-long=[delta,echo] n=2\n",
            "too-far-back=[alpha,bravo] n=2\n",
            "empty-src=[] n=0\n",
            "null-len=[bravo,charlie,delta,echo] n=4\n",
            "after-mutate-copy=[MUTATED,charlie] n=2\n",
            "source-intact=[alpha,bravo,charlie,delta,echo] n=5\n",
            "detached=[y,z] n=2\n",
            "nested=[charlie,delta] n=2\n",
            "long=200,300 first=w second=e\n",
        )
    );
}

/// Slicing an indexed string array in a loop must not leak or double-free.
///
/// The copy persists each `{pointer, length}` pair through `__rt_array_push_str`, so the result
/// owns bytes the source also owns a copy of. Getting that wrong in either direction is silent
/// at the value level — the strings still print — and only shows up as an allocation imbalance,
/// which is what this asserts: 200 iterations, every block released.
#[test]
fn test_array_slice_on_string_array_balances_allocations() {
    let out = compile_and_run_with_gc_stats(
        r#"<?php
for ($i = 0; $i < 200; $i++) {
    $src = [str_repeat("a", 40), str_repeat("b", 40), str_repeat("c", 40)];
    $cut = array_slice($src, 1, 2);
    if (count($cut) !== 2) { echo "BAD\n"; }
}
echo "ok\n";
"#,
    );
    assert!(out.stdout.contains("ok"), "program output: {:?}", out.stdout);
    let stats = out
        .stderr
        .lines()
        .find(|line| line.starts_with("GC: allocs="))
        .unwrap_or_else(|| panic!("no GC stats line in: {}", out.stderr));
    let (allocs, frees) = stats
        .trim_start_matches("GC: allocs=")
        .split_once(" frees=")
        .unwrap_or_else(|| panic!("unexpected GC stats shape: {stats}"));
    assert_eq!(allocs, frees, "array_slice on a string array leaked: {stats}");
}
