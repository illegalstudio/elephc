use super::*;

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
