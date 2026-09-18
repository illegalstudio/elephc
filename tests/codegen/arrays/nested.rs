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

// --- Issue #1096: an array-returning BUILTIN call as a literal element ---

/// Verifies an indexed literal keeps the arrays a builtin call returns (issue #1096).
///
/// `array_literal_element_type_for_ir` looks a callee up in `ctx.functions` and
/// `ctx.extern_functions`; a builtin is in neither, so the literal was stamped from the
/// syntactic fallback's `Int` and lowering inserted an `(int)` cast of the returned array to
/// match. `(int)` of a non-empty array is `1`, so every element became `int(1)` -- silently,
/// and independent of what the array actually held.
#[test]
fn test_issue_1096_indexed_literal_of_array_returning_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
$a = [1, 2, 3, 4, 5];
$c = [array_slice($a, 0, 2), array_slice($a, 2)];
echo count($c), ":", count($c[0]), ",", count($c[1]), ":", $c[0][1], ",", $c[1][2];
"#,
    );
    assert_eq!(out, "2:2,3:2,5");
}

/// Covers the other array-returning builtins that reached the same fallback.
///
/// `array_reverse` and `explode` are here because their first elements are `5` and `"a"`:
/// both rendered `int(1)` before the fix, which is what proves the stored value was
/// `(int)$array` rather than a mis-read first element.
#[test]
fn test_issue_1096_literal_elements_from_several_array_builtins() {
    let out = compile_and_run(
        r#"<?php
function values(): string { $a = [1, 2, 3]; $c = [array_values($a)]; return implode(",", $c[0]); }
function keys(): string { $a = [1, 2, 3]; $c = [array_keys($a)]; return implode(",", $c[0]); }
function merged(): string { $a = [1, 2]; $c = [array_merge($a, [3])]; return implode(",", $c[0]); }
function reversed(): string { $a = [1, 2, 3]; $c = [array_reverse($a)]; return implode(",", $c[0]); }
function split(): string { $c = [explode(",", "a,b")]; return implode("|", $c[0]); }
echo values(), ";", keys(), ";", merged(), ";", reversed(), ";", split();
"#,
    );
    assert_eq!(out, "1,2,3;0,1,2;1,2,3;3,2,1;a|b");
}

/// Verifies the associative twin keeps the array too, which was refused outright before.
#[test]
fn test_issue_1096_assoc_literal_value_from_an_array_returning_builtin() {
    let out = compile_and_run(
        r#"<?php
$a = [1, 2, 3, 4, 5];
$c = ["head" => array_slice($a, 0, 2), "tail" => array_values($a)];
echo count($c["head"]), ",", count($c["tail"]), ":", $c["head"][1], ",", $c["tail"][4];
"#,
    );
    assert_eq!(out, "2,5:2,5");
}

/// Verifies a `foreach` over such a literal binds the arrays, not scalars.
#[test]
fn test_issue_1096_foreach_over_a_literal_of_builtin_calls_binds_arrays() {
    let out = compile_and_run(
        r#"<?php
$a = [1, 2, 3, 4, 5];
$c = [array_slice($a, 0, 2), array_slice($a, 2)];
$total = 0;
foreach ($c as $row) { $total = $total + count($row); }
echo $total;
"#,
    );
    assert_eq!(out, "5");
}

/// Control: scalar-returning builtins must keep their own types, not gain an array stamp.
///
/// The fix reads `ctx.builtin_call_types`, which answers for every builtin, not only the
/// array-returning ones -- so these four are what pin it to reading the type rather than
/// assuming one.
#[test]
fn test_literal_elements_from_scalar_returning_builtins_keep_their_types() {
    let out = compile_and_run(
        r#"<?php
function counted(): string { $a = [1, 2, 3]; $c = [count($a), 7]; return $c[0] . "," . $c[1]; }
function upper(): string { $c = [strtoupper("a"), "b"]; return $c[0] . "," . $c[1]; }
function rooted(): string { $c = [sqrt(4.0), 1.5]; return $c[0] . "," . $c[1]; }
function member(): string { $a = [1, 2]; $c = [in_array(1, $a), false]; return var_export($c[0], true) . "," . var_export($c[1], true); }
echo counted(), ";", upper(), ";", rooted(), ";", member();
"#,
    );
    assert_eq!(out, "3,7;A,b;2,1.5;true,false");
}

/// Control: a user function returning `array` already worked through `ctx.functions`, and a
/// literal that mixes a call with a plain literal already widened correctly. Both must stay
/// correct -- they are the two shapes that hid the defect.
#[test]
fn test_literal_elements_from_a_user_function_and_a_mixed_literal_stay_correct() {
    let out = compile_and_run(
        r#"<?php
function mk(): array { return [1, 2]; }
$a = [1, 2, 3, 4, 5];
$u = [mk(), mk()];
$m = [array_slice($a, 0, 2), [9, 9]];
echo count($u[0]), ",", $u[1][1], ";", count($m), ",", $m[0][1], ",", $m[1][0];
"#,
    );
    assert_eq!(out, "2,2;2,2,9");
}
