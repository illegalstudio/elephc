//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of object suites, including nested indexed assoc direct, nested assoc indexed, and nested integer assoc in indexed.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

/// Compiles `[["name" => "Alice"]]` then accesses `$data[0]["name"]` directly via
/// nested indexed-then-assoc indexing. Verifies the chained subscript path produces
/// "Alice".
#[test]
fn test_nested_indexed_assoc_direct() {
    let out = compile_and_run(
        r#"<?php
$data = [["name" => "Alice"]];
echo $data[0]["name"];
"#,
    );
    assert_eq!(out, "Alice");
}

/// Compiles `["items" => [10, 20, 30]]`, extracts `$map["items"]` to a variable,
/// then accesses `$items[1]`. Verifies assoc-then-indexed subscript path yields 20.
#[test]
fn test_nested_assoc_indexed() {
    let out = compile_and_run(
        r#"<?php
$map = ["items" => [10, 20, 30]];
$items = $map["items"];
echo $items[1];
"#,
    );
    assert_eq!(out, "20");
}

/// Compiles `[["math" => 90, "eng" => 85]]`, extracts `$scores[0]` to a variable,
/// then accesses both string keys. Verifies multi-key assoc inside indexed array.
#[test]
fn test_nested_int_assoc_in_indexed() {
    let out = compile_and_run(
        r#"<?php
$scores = [["math" => 90, "eng" => 85]];
$s = $scores[0];
echo $s["math"] . "|" . $s["eng"];
"#,
    );
    assert_eq!(out, "90|85");
}

/// Compiles an indexed array of assoc arrays, iterates with a for loop,
/// extracts each inner assoc to a variable, and concatenates name|email pairs.
/// Verifies loop index bounds, variable extraction, and string concatenation.
#[test]
fn test_nested_string_assoc_loop() {
    let out = compile_and_run(
        r#"<?php
$contacts = [
    ["name" => "Alice", "email" => "alice@test"],
    ["name" => "Bob", "email" => "bob@test"]
];
for ($i = 0; $i < 2; $i++) {
    $c = $contacts[$i];
    echo $c["name"] . "|" . $c["email"] . "\n";
}
"#,
    );
    assert_eq!(out, "Alice|alice@test\nBob|bob@test\n");
}

/// Compiles `["fruits" => ["apple", "banana"], "vegs" => ["carrot", "pea"]]`,
/// extracts `$groups["fruits"]` to a variable, then accesses both indexed slots.
/// Verifies assoc-of-indexed nested structure.
#[test]
fn test_nested_assoc_of_indexed() {
    let out = compile_and_run(
        r#"<?php
$groups = ["fruits" => ["apple", "banana"], "vegs" => ["carrot", "pea"]];
$f = $groups["fruits"];
echo $f[0] . "|" . $f[1];
"#,
    );
    assert_eq!(out, "apple|banana");
}

/// Compiles a `make_user` function returning an assoc array, builds an indexed array
/// by appending two calls, then iterates with count-based for loop. Verifies
/// function return, array push, and loop variable extraction from nested structure.
#[test]
fn test_nested_dynamic_building() {
    let out = compile_and_run(
        r#"<?php
function make_user($name, $email) {
    return ["name" => $name, "email" => $email];
}
$users = [];
$users[] = make_user("Alice", "a@t");
$users[] = make_user("Bob", "b@t");
for ($i = 0; $i < count($users); $i++) {
    $u = $users[$i];
    echo $u["name"] . "|" . $u["email"] . "\n";
}
"#,
    );
    assert_eq!(out, "Alice|a@t\nBob|b@t\n");
}

/// Compiles `parse_row` that calls `explode("|", $line)` and returns an assoc array
/// built from the parts. Verifies explode integration with assoc return and
/// nested string access via returned value.
#[test]
fn test_nested_explode_to_assoc() {
    let out = compile_and_run(
        r#"<?php
function parse_row($line) {
    $parts = explode("|", $line);
    return ["name" => $parts[0], "email" => $parts[1]];
}
$r = parse_row("Alice|alice@test");
echo $r["name"] . " <" . $r["email"] . ">";
"#,
    );
    assert_eq!(out, "Alice <alice@test>");
}

/// Compiles an indexed array of assoc arrays, iterates with foreach, and accesses
/// the "name" key on each iteration variable. Verifies foreach loop over nested
/// assoc without explicit index variable.
#[test]
fn test_nested_foreach_of_assoc() {
    let out = compile_and_run(
        r#"<?php
$people = [["name" => "Alice"], ["name" => "Bob"]];
foreach ($people as $p) {
    echo $p["name"] . " ";
}
"#,
    );
    assert_eq!(out, "Alice Bob ");
}

/// Compiles a class `Item` with a constructor, creates `["items" => [new Item(...), new Item(...)]]`,
/// extracts the inner array, indexes it, and accesses an object property. Verifies
/// objects stored inside nested assoc-of-indexed structures are materialized correctly.
#[test]
fn test_nested_objects_in_assoc() {
    let out = compile_and_run(
        r#"<?php
class Item { public $name;
    public function __construct($n) { $this->name = $n; }
}
$data = ["items" => [new Item("Sword"), new Item("Shield")]];
$items = $data["items"];
$first = $items[0];
echo $first->name;
"#,
    );
    assert_eq!(out, "Sword");
}

/// Compiles a `classify` function using switch with string return values across all
/// branches (case 0/1/default). Calls it three times and concatenates results
/// separated by spaces. Verifies switch fallthrough and string return routing.
#[test]
fn test_switch_return_string() {
    let out = compile_and_run(
        r#"<?php
function classify($n) {
    switch ($n % 3) {
        case 0: return "fizz";
        case 1: return "buzz";
        default: return "none";
    }
}
$r = classify(0);
echo $r . " ";
$r = classify(1);
echo $r . " ";
$r = classify(2);
echo $r;
"#,
    );
    assert_eq!(out, "fizz buzz none");
}

/// Compiles a `score` function using switch with integer return values across all
/// branches (case 1/2/3/default). Calls it four times and concatenates results
/// with pipe delimiters. Verifies switch with integer returns and default branch.
#[test]
fn test_switch_return_int() {
    let out = compile_and_run(
        r#"<?php
function score($grade) {
    switch ($grade) {
        case 1: return 100;
        case 2: return 80;
        case 3: return 60;
        default: return 0;
    }
}
echo score(1) . "|" . score(2) . "|" . score(3) . "|" . score(9);
"#,
    );
    assert_eq!(out, "100|80|60|0");
}
