//! Purpose:
//! Covers declared PHP array column extraction and ownership of selected values.
//!
//! Called from:
//! - The runtime GC codegen integration suite.
//!
//! Key details:
//! - Both packed and keyed outer arrays may hold boxed rows.
//! - Missing columns are skipped, present nulls survive, and results outlive their sources.

use crate::support::*;

/// Keyed boxed rows preserve present nulls, skip non-arrays and keep extracted strings alive.
#[test]
fn test_declared_array_column_presence_and_string_owners() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function columnRows(): array {
    return ["first" => ["col" => str_repeat("k", 4)], "missing" => ["other" => 9],
        "null" => ["col" => null], "scalar" => 42];
}
function columnValues(array $rows, string $key): array { return array_column($rows, $key); }
$rows = columnRows();
$values = columnValues($rows, "col");
$empty = columnValues($rows, "absent");
unset($rows);
echo count($values), ":", $values[0], ":", is_null($values[1]) ? "null" : "wrong", ":", count($empty);
unset($values, $empty);
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "2:kkkk:null:0", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Numeric-string columns work on packed rows, and nested values retain independent owners.
#[test]
fn test_declared_array_column_numeric_keys_and_nested_owners() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function numberedRows(): array { return [["left", "right"], ["last"]]; }
function nestedColumnRows(): array {
    return ["first" => ["col" => ["k" => "kept"]], "second" => ["col" => ["k" => "second"]]];
}
$first = array_column(numberedRows(), "0");
$second = array_column(numberedRows(), "1");
$source = nestedColumnRows();
$nested = array_column($source, "col");
$copy = $nested;
$nested[0]["k"] = "changed";
echo implode(",", $first), "|", implode(",", $second), "|", $source["first"]["col"]["k"], "|";
unset($source);
echo $nested[0]["k"], ":", $nested[1]["k"], "|", $copy[0]["k"];
unset($first, $second, $nested, $copy);
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "left,last|right|kept|changed:second|kept", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Result growth transfers every column cell and releases object payloads only after the result.
#[test]
fn test_declared_array_column_growth_and_object_owners() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class ColumnOwner { public function __destruct() { echo "released"; } }
function growthRows(): array {
    return [["id" => 0], ["id" => 1], ["id" => 2], ["id" => 3], ["id" => 4],
        ["id" => 5], ["id" => 6], ["id" => 7], ["id" => 8], ["id" => 9]];
}
function objectRows(): array { return [["obj" => new ColumnOwner()]]; }
$values = array_column(growthRows(), "id");
echo count($values), ":", $values[9], "|";
$objects = array_column(objectRows(), "obj");
echo get_class($objects[0]), "|";
unset($objects, $values);
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "10:9|ColumnOwner|released", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
