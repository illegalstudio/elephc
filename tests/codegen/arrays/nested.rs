use crate::support::*;

// --- Phase 14: Multi-dimensional arrays ---

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

