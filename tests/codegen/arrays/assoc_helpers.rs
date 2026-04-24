use crate::support::*;

// --- Associative array function tests ---

#[test]
fn test_assoc_array_key_exists() {
    let out = compile_and_run(
        r#"<?php
$m = ["name" => "Alice", "age" => "30"];
if (array_key_exists("name", $m)) { echo "yes"; }
if (array_key_exists("missing", $m)) { echo "bad"; } else { echo "no"; }
"#,
    );
    assert_eq!(out, "yesno");
}

#[test]
fn test_assoc_in_array_str() {
    let out = compile_and_run(
        r#"<?php
$m = ["a" => "apple", "b" => "banana"];
if (in_array("apple", $m)) { echo "yes"; }
if (in_array("cherry", $m)) { echo "bad"; } else { echo "no"; }
"#,
    );
    assert_eq!(out, "yesno");
}

#[test]
fn test_assoc_in_array_int() {
    let out = compile_and_run(
        r#"<?php
$m = ["x" => 10, "y" => 20];
if (in_array(10, $m)) { echo "yes"; }
if (in_array(99, $m)) { echo "bad"; } else { echo "no"; }
"#,
    );
    assert_eq!(out, "yesno");
}

#[test]
fn test_assoc_array_search_str() {
    let out = compile_and_run(
        r#"<?php
$m = ["first" => "Alice", "second" => "Bob"];
$key = array_search("Bob", $m);
echo $key;
"#,
    );
    assert_eq!(out, "second");
}

#[test]
fn test_assoc_array_keys() {
    let out = compile_and_run(
        r#"<?php
$m = ["x" => 1, "y" => 2];
$keys = array_keys($m);
$n = count($keys);
for ($i = 0; $i < $n; $i++) {
    echo $keys[$i] . " ";
}
"#,
    );
    assert_eq!(out, "x y ");
}

#[test]
fn test_assoc_array_search_returns_first_inserted_matching_key() {
    let out = compile_and_run(
        r#"<?php
$m = ["first" => "same", "second" => "same", "third" => "other"];
$key = array_search("same", $m);
echo $key;
echo "|";
echo count($m);
"#,
    );
    assert_eq!(out, "first|3");
}

#[test]
fn test_assoc_array_values_str() {
    let out = compile_and_run(
        r#"<?php
$m = ["a" => "one", "b" => "two"];
$vals = array_values($m);
$n = count($vals);
for ($i = 0; $i < $n; $i++) {
    echo $vals[$i] . " ";
}
"#,
    );
    assert_eq!(out, "one two ");
}

#[test]
fn test_assoc_array_values_int() {
    let out = compile_and_run(
        r#"<?php
$m = ["a" => 10, "b" => 20, "c" => 30];
$vals = array_values($m);
echo $vals[0] + $vals[1] + $vals[2];
"#,
    );
    assert_eq!(out, "60");
}

#[test]
fn test_assoc_array_mixed_foreach() {
    let out = compile_and_run(
        r#"<?php
$m = ["id" => 7, "name" => "Alice", "score" => 12];
foreach ($m as $key => $value) {
    echo $key;
    echo "=";
    echo $value;
    echo ";";
}
"#,
    );
    assert_eq!(out, "id=7;name=Alice;score=12;");
}

#[test]
fn test_assoc_array_values_mixed() {
    let out = compile_and_run(
        r#"<?php
$m = ["id" => 7, "name" => "Alice", "score" => 12];
$vals = array_values($m);
$n = count($vals);
for ($i = 0; $i < $n; $i++) {
    echo $vals[$i];
    echo ",";
}
"#,
    );
    assert_eq!(out, "7,Alice,12,");
}

#[test]
fn test_assoc_in_array_mixed() {
    let out = compile_and_run(
        r#"<?php
$m = ["id" => 7, "name" => "Alice", "score" => 12];
if (in_array("Alice", $m)) { echo "name"; }
if (in_array(12, $m)) { echo " score"; }
if (!in_array("Bob", $m)) { echo " missing"; }
"#,
    );
    assert_eq!(out, "name score missing");
}

#[test]
fn test_assoc_array_search_mixed() {
    let out = compile_and_run(
        r#"<?php
$m = ["id" => 7, "name" => "Alice", "score" => 12];
echo array_search("Alice", $m);
echo ":";
echo array_search(12, $m);
"#,
    );
    assert_eq!(out, "name:score");
}

#[test]
fn test_assoc_array_access_mixed_echo() {
    let out = compile_and_run(
        r#"<?php
$m = ["id" => 7, "name" => "Alice", "score" => 12];
echo $m["name"];
"#,
    );
    assert_eq!(out, "Alice");
}

#[test]
fn test_gc_assoc_array_values_borrowed_array_survives_source_unset() {
    let out = compile_and_run(
        r#"<?php
$map = ["nums" => [7, 8, 9]];
$vals = array_values($map);
unset($map);
$saved = $vals[0];
echo $saved[1];
"#,
    );
    assert_eq!(out, "8");
}
