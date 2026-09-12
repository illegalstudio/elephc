//! Purpose:
//! Regression coverage for key sorting nested array values stored in heterogeneous parents.
//!
//! Called from:
//! - `cargo test --test codegen_tests` through `tests/codegen/arrays/indexed.rs`.
//!
//! Key details:
//! - Both key-sort directions must share runtime tag validation, COW separation, and write-back.
//! - Local and property-backed parents must accept the same nested Mixed lvalue shapes.
//! - Scalar and missing child cells must raise builtin-specific PHP `TypeError` diagnostics.

use super::*;
use crate::support::{compile_and_run_tagged, compile_and_run_with_heap_debug};

/// Declared array parents retain COW, named-call evaluation and temporary ownership across key sorts.
#[test]
fn test_key_sorts_declared_array_children_preserve_aliases_and_retire_temporaries() {
    let source = r#"<?php
class DeclaredNestedSortOwner {
    public array $rows = [["b" => 2, "a" => 1], "keep"];
    public static array $shared = ["row" => [8, 9], "keep" => 7];
}
function nestedSortIndex(int &$calls): int { $calls++; return 0; }
function reverseDeclaredNestedCopy(array $rows): void {
    krsort($rows["row"]);
    echo implode(",", array_keys($rows["row"])), "|";
}
for ($i = 0; $i < 3; $i++) {
    $owner = new DeclaredNestedSortOwner();
    $before = $owner->rows;
    $calls = 0;
    echo \KsOrT(array: $owner->rows[nestedSortIndex($calls)]) ? "yes|" : "no|";
    echo implode(",", array_keys($owner->rows[0])), "|";
    echo implode(",", array_keys($before[0])), "|", $calls, "|";
    $staticBefore = DeclaredNestedSortOwner::$shared;
    krsort(DeclaredNestedSortOwner::$shared["row"]);
    echo implode(",", array_keys(DeclaredNestedSortOwner::$shared["row"])), "|";
    echo implode(",", array_keys($staticBefore["row"])), "|";
    ksort(DeclaredNestedSortOwner::$shared["row"]);
    reverseDeclaredNestedCopy(DeclaredNestedSortOwner::$shared);
    echo implode(",", array_keys(DeclaredNestedSortOwner::$shared["row"])), "|";
    echo $owner->rows[1], "|", DeclaredNestedSortOwner::$shared["keep"], "|";
    unset($owner, $before, $staticBefore);
}
"#;
    let expected = "yes|a,b|b,a|1|1,0|0,1|1,0|0,1|keep|7|".repeat(3);
    let output = compile_and_run_with_heap_debug(source);
    assert!(output.success, "stdout={:?}\nstderr={}", output.stdout, output.stderr);
    assert_eq!(output.stdout, expected, "{}", output.stderr);
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
    assert_eq!(compile_and_run_tagged(source), expected);
}

/// Invalid declared-array child values throw before parent write-back and leave their siblings intact.
#[test]
fn test_key_sorts_declared_array_scalar_child_throws_without_mutating_parent() {
    let source = r#"<?php
class InvalidDeclaredNestedSortOwner {
    public array $rows = ["bad" => 7, "keep" => ["b" => 2, "a" => 1]];
}
function invalidDeclaredNestedSort(array &$rows, string $key): void { krsort($rows[$key]); }
for ($i = 0; $i < 3; $i++) {
    $owner = new InvalidDeclaredNestedSortOwner();
    try { ksort($owner->rows["bad"]); }
    catch (TypeError $error) { echo "ascending|"; unset($error); }
    $rows = $owner->rows;
    try { invalidDeclaredNestedSort($rows, "bad"); }
    catch (TypeError $error) { echo "descending|"; unset($error); }
    echo $owner->rows["bad"], "|", $rows["bad"], "|";
    echo implode(",", array_keys($rows["keep"])), "|";
    unset($owner, $rows);
}
"#;
    let expected = "ascending|descending|7|7|b,a|".repeat(3);
    let output = compile_and_run_with_heap_debug(source);
    assert!(output.success, "stdout={:?}\nstderr={}", output.stdout, output.stderr);
    assert_eq!(output.stdout, expected, "{}", output.stderr);
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
    assert_eq!(compile_and_run_tagged(source), expected);
}

/// Verifies `ksort()` accepts a nested hash stored in a Mixed packed-parent cell.
#[test]
fn test_ksort_nested_hash_of_mixed_packed_parent() {
    let out = compile_and_run(
        r#"<?php
$grid = [["b" => 2, "a" => 1], "sentinel"];
$index = 0;
echo ksort(array: $grid[$index]) ? "true|" : "false|";
foreach ($grid[0] as $key => $value) {
    echo $key . ":" . $value . ",";
}
echo "|" . $grid[1];
"#,
    );
    assert_eq!(out, "true|a:1,b:2,|sentinel");
}

/// Verifies both key-sort directions accept a nested hash stored in a Mixed
/// value cell of a heterogeneous associative parent.
#[test]
fn test_key_sorts_nested_hash_of_mixed_associative_parent() {
    let out = compile_and_run(
        r#"<?php
$matrix = ["row" => ["b" => 2, "a" => 1], "sentinel" => 7];
echo ksort($matrix["row"]) ? "true|" : "false|";
foreach ($matrix["row"] as $key => $value) { echo $key . $value; }
echo "|";
echo krsort($matrix["row"]) ? "true|" : "false|";
foreach ($matrix["row"] as $key => $value) { echo $key . $value; }
echo "|" . $matrix["sentinel"];
"#,
    );
    assert_eq!(out, "true|a1b2|true|b2a1|7");
}

/// Verifies `$this` and external object property parents use the same nested
/// Mixed sorting path as locals, including parent-level copy-on-write.
#[test]
fn test_key_sorts_nested_mixed_object_property_parents() {
    let out = compile_and_run(
        r#"<?php
class GridOwner {
    public function __construct(public array $grid, public array $rows) {}

    public function sortGrid(): void {
        ksort($this->grid[0]);
    }
}

function reverseRow(GridOwner $owner, int $key): void {
    krsort($owner->rows[$key]);
}

$owner = new GridOwner(
    [["b" => 2, "a" => 1], "sentinel"],
    [["a" => 1, "b" => 2], 7],
);
$original = $owner->grid;
$owner->sortGrid();
foreach ($owner->grid[0] as $key => $value) { echo $key . $value; }
echo "|";
foreach ($original[0] as $key => $value) { echo $key . $value; }
echo "|";
$key = 0;
reverseRow($owner, $key);
foreach ($owner->rows[$key] as $name => $value) { echo $name . $value; }
echo "|" . $owner->rows[1];
"#,
    );
    assert_eq!(out, "a1b2|b2a1|b2a1|7");
}

/// Verifies `ksort()` treats a nested packed child as an ascending-key no-op returning true.
#[test]
fn test_ksort_nested_packed_child_of_mixed_parent_is_noop() {
    let out = compile_and_run(
        r#"<?php
$grid = [[3, 1, 2], "sentinel"];
echo ksort($grid[0]) ? "true|" : "false|";
foreach ($grid[0] as $key => $value) {
    echo $key . ":" . $value . ",";
}
"#,
    );
    assert_eq!(out, "true|0:3,1:1,2:2,");
}

/// Verifies nested `ksort()` detaches a shared Mixed-parent cell before sorting its hash.
#[test]
fn test_ksort_cow_splits_mixed_packed_parent() {
    let out = compile_and_run(
        r#"<?php
$grid = [["b" => 2, "a" => 1], "sentinel"];
$original = $grid;
ksort($grid[0]);
foreach ($grid[0] as $key => $value) {
    echo $key . ":" . $value . ",";
}
echo "|";
foreach ($original[0] as $key => $value) {
    echo $key . ":" . $value . ",";
}
"#,
    );
    assert_eq!(out, "a:1,b:2,|b:2,a:1,");
}

/// Verifies a scalar Mixed child reaches a controlled `ksort()` array TypeError.
#[test]
fn test_ksort_scalar_child_of_mixed_parent_reports_type_error() {
    let out = compile_and_run_capture(
        r#"<?php
$grid = [["b" => 2, "a" => 1], "sentinel"];
ksort($grid[1]);
"#,
    );
    assert!(!out.success, "scalar nested value should fail");
    assert!(
        out.stdout.contains("ksort()")
            && out.stdout.contains("Argument #1")
            && out.stdout.contains("array"),
        "expected a controlled ksort array TypeError, got: {}",
        out.stdout,
    );
}

/// Verifies a missing Mixed child reaches a controlled `ksort()` array TypeError.
#[test]
fn test_ksort_missing_child_of_mixed_parent_reports_type_error() {
    let out = compile_and_run_capture(
        r#"<?php
$grid = [["b" => 2, "a" => 1], "sentinel"];
ksort($grid[9]);
"#,
    );
    assert!(!out.success, "missing nested value should fail");
    assert!(
        out.stdout.contains("ksort()")
            && out.stdout.contains("Argument #1")
            && out.stdout.contains("array"),
        "expected a controlled ksort array TypeError, got: {}",
        out.stdout,
    );
}
