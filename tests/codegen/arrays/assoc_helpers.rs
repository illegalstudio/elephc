//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of array associative-array helper builtins, including array key exists, in array string, and in array integer.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use crate::support::*;

// --- Associative array function tests ---

#[test]
fn test_assoc_array_key_exists() {
    // Verifies array_key_exists() returns true for present keys and false for absent ones.
    // Fixture: two-element string-keyed assoc array, two lookups (one present, one absent).
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
    // Verifies in_array() with string needle finds string values in assoc array.
    // Fixture: two-element string-keyed assoc array, string needle "apple" present, "cherry" absent.
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
    // Verifies in_array() with integer needle finds integer values in assoc array.
    // Fixture: two-element string-keyed assoc array, integer needle 10 present, 99 absent.
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
    // Verifies array_search() returns the key for a found string value.
    // Fixture: two-element string-keyed assoc array, searches for "Bob".
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
fn test_assoc_array_search_returns_integer_and_string_keys() {
    // Verifies array_search() returns integer keys as integers and string keys as strings.
    // Fixture: assoc array with int key `10` and string key `"02"`, each holding a distinct value.
    let out = compile_and_run(
        r#"<?php
$m = [10 => "Alice", "02" => "Bob"];
echo array_search("Alice", $m);
echo "|";
echo array_search("Bob", $m);
"#,
    );
    assert_eq!(out, "10|02");
}

#[test]
fn test_assoc_array_search_integer_key_matches_declared_union_return() {
    // Verifies array_search() with an integer key fits the int|bool return type annotation.
    // Fixture: int-keyed assoc array with declared return type `int|bool` on the wrapper function.
    let out = compile_and_run(
        r#"<?php
function find_key(): int|bool {
    $m = [10 => "Alice", 20 => "Bob"];
    return array_search("Alice", $m);
}

echo find_key();
"#,
    );
    assert_eq!(out, "10");
}

#[test]
fn test_assoc_array_search_not_found_is_strict_false() {
    // Verifies array_search() returns strictly false (not 0 or empty string) when value is absent.
    // Fixture: two-element string-keyed assoc array, searches for "Carol" which is not present.
    let out = compile_and_run(
        r#"<?php
$m = ["first" => "Alice", "second" => "Bob"];
echo array_search("Carol", $m) === false ? "miss" : "hit";
"#,
    );
    assert_eq!(out, "miss");
}

#[test]
fn test_assoc_array_keys() {
    // Verifies array_keys() returns all keys of an assoc array in insertion order.
    // Fixture: two-element string-keyed assoc array, iterates and echoes keys with spaces.
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
fn test_assoc_array_keys_preserves_integer_and_string_keys() {
    // Verifies array_keys() preserves integer key `1` and string key `"02"` as distinct types.
    // Fixture: assoc array with mixed int/string keys; echoes both keys separated by `|`.
    let out = compile_and_run(
        r#"<?php
$m = [1 => "one", "02" => "two"];
$keys = array_keys($m);
echo $keys[0] . "|" . $keys[1];
"#,
    );
    assert_eq!(out, "1|02");
}

#[test]
fn test_assoc_array_search_returns_first_inserted_matching_key() {
    // Verifies array_search() returns the first-matching key in insertion order, not the last.
    // Fixture: three-element assoc array where "same" maps to two keys; confirms only first is returned and array size is unchanged.
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
    // Verifies array_values() returns all string values of a string-keyed assoc array.
    // Fixture: two-element string-keyed assoc array, iterates and echoes values with spaces.
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
    // Verifies array_values() returns integer values and they can be used in arithmetic.
    // Fixture: three-element string-keyed assoc array with integer values; sums them to confirm int type.
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
    // Verifies foreach over a mixed-type assoc array yields correct key/value pairs.
    // Fixture: three-element assoc array with mixed int and string values, echoes key=value; pairs.
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
    // Verifies array_values() on a mixed-type assoc array returns values in insertion order.
    // Fixture: three-element assoc array with mixed int/string values, echoes val,val,... format.
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
    // Verifies in_array() finds both string and integer values in a mixed-type assoc array.
    // Fixture: three-element assoc array with mixed types, three lookups: string present, int present, string absent.
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
    // Verifies array_search() on a mixed-type assoc array returns correct key for string and int values.
    // Fixture: three-element assoc array with mixed types, searches for "Alice" (string value) and 12 (int value).
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
    // Verifies direct array access via string key on a mixed-type assoc array.
    // Fixture: three-element assoc array with mixed types, accesses $m["name"] and echoes result.
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
    // Verifies array_values() produces a borrowed reference that survives unset of the source array.
    // Regression: array_values must not copy-or-free source data while borrowed; $vals[0] must remain valid after unset($map).
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
