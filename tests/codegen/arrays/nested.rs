//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of array suites, including nested array create access, nested array count, and nested array push.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use crate::support::*;

// --- Phase 14: Multi-dimensional arrays ---

/// Compiles a 2D numeric array literal and verifies indexed access to all four elements.
#[test]
fn test_nested_array_create_access() {
    let out = compile_and_run(
        r#"<?php
$a = [[1, 2], [3, 4]];
echo $a[0][0] . " " . $a[0][1] . " " . $a[1][0] . " " . $a[1][1];
"#,
    );
    assert_eq!(out, "1 2 3 4");
}

/// Verifies `count()` on a 2D array returns the outer element count and the inner sub-array length.
#[test]
fn test_nested_array_count() {
    let out = compile_and_run(
        r#"<?php
$a = [[10, 20], [30, 40], [50, 60]];
echo count($a) . " " . count($a[0]);
"#,
    );
    assert_eq!(out, "3 2");
}

/// Appends a new sub-array to a 2D array via `[]` and confirms the outer count incremented and the new sub-array is accessible.
#[test]
fn test_nested_array_push() {
    let out = compile_and_run(
        r#"<?php
$a = [[1, 2]];
$a[] = [3, 4];
echo count($a) . " " . $a[1][0];
"#,
    );
    assert_eq!(out, "2 3");
}

/// Exercises nested `foreach` iteration over a 2D array, confirming each scalar element is visited in row-major order.
#[test]
fn test_nested_array_foreach() {
    let out = compile_and_run(
        r#"<?php
$matrix = [[1, 2], [3, 4]];
foreach ($matrix as $row) {
    foreach ($row as $v) {
        echo $v . " ";
    }
}
"#,
    );
    assert_eq!(out, "1 2 3 4 ");
}

/// Tests three levels of array nesting (`[[[1]]]`), verifying that evaluation order correctly traverses to the innermost scalar.
#[test]
fn test_nested_array_3_levels() {
    let out = compile_and_run(
        r#"<?php
$a = [[[1]]];
echo $a[0][0][0];
"#,
    );
    assert_eq!(out, "1");
}

/// Constructs a 2D array of string values and asserts that indexed access and string concatenation produce the expected output.
#[test]
fn test_nested_array_string_elements() {
    let out = compile_and_run(
        r#"<?php
$a = [["hello", "world"], ["foo", "bar"]];
echo $a[0][0] . " " . $a[1][1];
"#,
    );
    assert_eq!(out, "hello bar");
}

/// Tests `array_column()` extracting a named string key from an array of associative rows, confirming the result count is correct.
#[test]
fn test_array_column() {
    let out = compile_and_run(
        r#"<?php
$users = [
    ["name" => "Alice", "age" => "30"],
    ["name" => "Bob", "age" => "25"],
    ["name" => "Charlie", "age" => "35"],
];
$names = array_column($users, "name");
echo count($names);
"#,
    );
    assert_eq!(out, "3");
}

/// Exercises `array_column()` on rows containing mixed (string and int) values, then iterates both result arrays to confirm ordering and values are preserved.
#[test]
fn test_array_column_mixed_row_values() {
    let out = compile_and_run(
        r#"<?php
$users = [
    ["name" => "Ada", "score" => 10],
    ["name" => "Linus", "score" => 12],
    ["name" => "Grace", "score" => 8],
];
$names = array_column($users, "name");
$scores = array_column($users, "score");
foreach ($names as $name) {
    echo $name . " ";
}
echo "|";
foreach ($scores as $score) {
    echo $score . " ";
}
"#,
    );
    assert_eq!(out, "Ada Linus Grace |10 12 8 ");
}

/// Regression test for GC balance: after `array_column()` on mixed-type rows with all three arrays subsequently `unset`, allocations must equal frees.
#[test]
fn test_array_column_mixed_row_values_balances_gc_stats() {
    let baseline = compile_and_run_with_gc_stats("<?php");
    let out = compile_and_run_with_gc_stats(
        r#"<?php
$users = [
    ["name" => "Ada", "score" => 10],
    ["name" => "Linus", "score" => 12],
    ["name" => "Grace", "score" => 8],
];
$names = array_column($users, "name");
$scores = array_column($users, "score");
unset($names);
unset($scores);
unset($users);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    let (baseline_allocs, baseline_frees) = parse_gc_stats(&baseline.stderr);
    let (allocs, frees) = parse_gc_stats(&out.stderr);
    assert_eq!(allocs - baseline_allocs, frees - baseline_frees);
}

/// An assoc literal nested inside an assoc literal keeps its own element type.
///
/// The outer literal stamps a value type on its hash, and for a nested literal it used to take
/// the context-free syntactic guess, which types every variable `int`. The outer hash therefore
/// claimed to hold `array<string, int>` while the inner hash really held `array<string>`, and a
/// read through both levels returned the inner array POINTER read back as an integer -- the
/// `int(...)` symptom in issue #984. `$v` has to come from a local: a literal spelled out in
/// place is the one case the syntactic guess gets right on its own.
#[test]
fn test_nested_assoc_literal_keeps_a_local_arrays_element_type() {
    let out = compile_and_run(
        r#"<?php
$v = ["p", "q"];
$a = ["outer" => ["inner" => $v]];
var_dump($a["outer"]["inner"][1]);
"#,
    );
    assert_eq!(out, "string(1) \"q\"\n");
}

/// The same nesting through an intermediate local, and with an int-valued inner array.
///
/// Reading `$a["outer"]` into its own local first goes through a different read path than the
/// chained `$a["outer"]["inner"]` above, but both consume the same fabricated stamp, so both
/// answered the pointer-as-integer before the fix.
#[test]
fn test_nested_assoc_literal_element_type_survives_an_intermediate_local() {
    let out = compile_and_run(
        r#"<?php
$s = ["p", "q"];
$a = ["outer" => ["inner" => $s]];
$mid = $a["outer"];
echo $mid["inner"][1], "|", count($mid["inner"]);
"#,
    );
    assert_eq!(out, "q|2");
}

/// An INDEXED outer literal was always correct, and has to stay that way.
///
/// `array_literal_element_type_for_ir` already carried the nested-literal arms that the
/// associative sibling was missing, which is exactly why `[["k" => $v]]` worked while
/// `["j" => ["k" => $v]]` did not. This pins the working half against a later change that
/// unifies the two.
#[test]
fn test_indexed_outer_literal_still_types_a_nested_assoc_literal() {
    let out = compile_and_run(
        r#"<?php
$v = ["p", "q"];
$g = [["inner" => $v]];
echo $g[0]["inner"][1], "|", count($g[0]["inner"]);
"#,
    );
    assert_eq!(out, "q|2");
}

/// A nested literal holding a loop-grown array, iterated rather than indexed.
///
/// `foreach` builds its iterator from the same stamped element type, so a fabricated `int`
/// value type made the loop read scalars out of an array pointer. The loop-grown source also
/// covers the case where the element type is only known from the local's inferred storage.
#[test]
fn test_nested_assoc_literal_array_iterates_its_real_elements() {
    let out = compile_and_run(
        r#"<?php
$items = [];
for ($i = 0; $i < 3; $i++) { $items[] = "e" . $i; }
$doc = ["body" => ["items" => $items]];
$seen = "";
foreach ($doc["body"]["items"] as $it) { $seen .= $it . ","; }
echo $seen, "|", count($doc["body"]["items"]);
"#,
    );
    assert_eq!(out, "e0,e1,e2,|3");
}

/// Verifies that `array_column()` creating copied sub-arrays survives `unset` of the source rows, and that individual nested elements are still accessible.
#[test]
fn test_gc_array_column_borrowed_array_survives_source_unset() {
    let out = compile_and_run(
        r#"<?php
$rows = [
    ["nums" => [4, 5]],
    ["nums" => [6, 7]],
];
$cols = array_column($rows, "nums");
unset($rows);
$first = $cols[0];
$second = $cols[1];
echo $first[1] . "|" . $second[0];
"#,
    );
    assert_eq!(out, "5|6");
}
