//! Purpose:
//! Regression coverage for PHP 8 `array_column()`: the `$index_key` argument, a `null`
//! `$column_key` (whole rows), integer column keys, object rows, and illegal-key TypeErrors.
//!
//! Called from:
//! - The native codegen suite's array module.
//!
//! Key details:
//! - Every fixture runs 25 iterations inside a function under `--heap-debug`, so a value the
//!   result keeps without owning it (or a partial result an error path forgets) shows up as a
//!   leak or a use-after-free. Expected stdout is reference PHP 8.5's.
//! - PHP 8.5 also prints "Using null as an array offset is deprecated" for a `null` index value;
//!   elephc never emits that deprecation for array offsets, so fixtures compare stdout only.

use crate::support::compile_and_run_with_heap_debug;

/// Asserts a heap-debug run succeeded, printed `expected`, and released every allocation.
fn assert_clean(source: &str, expected: &str) {
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, expected, "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Index values convert like PHP array keys; missing index keys append with the next integer
/// key, duplicates keep the first position with the last value, and `null` keeps whole rows.
#[test]
fn test_array_column_index_key_converts_like_array_keys() {
    assert_clean(
        r#"<?php
function rows(int $i): array {
    return [["id" => 1, "n" => "a" . $i], ["id" => "2", "n" => "b"], ["id" => "x", "n" => "c"],
        ["id" => true, "n" => "e"], ["id" => false, "n" => "f"], ["id" => null, "n" => "g"],
        ["n" => "h"], ["id" => "07", "n" => "i"], ["id" => 2, "n" => "j"]];
}
function run(int $i): string {
    $byId = array_column(rows($i), "n", "id");
    $out = json_encode($byId);
    $whole = array_column(rows($i), null, "id");
    $out .= "|" . json_encode(array_keys($whole)) . $whole["x"]["n"];
    return $out;
}
$out = "";
for ($i = 0; $i < 25; $i++) { $out = run($i); }
echo $out;
"#,
        r#"{"1":"e","2":"j","x":"c","0":"f","":"g","3":"h","07":"i"}|[1,2,"x",0,"",3,"07"]c"#,
    );
}

/// Integer column keys read list rows, a `null` column keeps scalar elements, rows lacking the
/// column are skipped, and numeric-string column keys address integer row keys.
#[test]
fn test_array_column_integer_and_null_columns() {
    assert_clean(
        r#"<?php
function pairs(int $i): array { return [[10, "ten" . $i], [20, "twenty"], [30]]; }
function run(int $i): string {
    $second = array_column(pairs($i), 1);
    $keyed = array_column(pairs($i), 1, 0);
    $rows = array_column(pairs($i), null, 0);
    $mixed = array_column([["a" => 1], "scalar", 5, ["a" => null], ["b" => 2]], "a");
    $all = array_column([1, ["k" => 2], "x" . $i], null);
    $numeric = array_column([["a" => 1, 5 => "five"], [5 => "cinq"]], "5");
    return json_encode($second) . json_encode($keyed) . json_encode(array_keys($rows))
        . json_encode($mixed) . json_encode($all) . json_encode($numeric);
}
$out = "";
for ($i = 0; $i < 25; $i++) { $out = run($i); }
echo $out;
"#,
        r#"["ten24","twenty"]{"10":"ten24","20":"twenty"}[10,20,30][1,null][1,{"k":2},"x24"]["five","cinq"]"#,
    );
}

/// Object rows expose public declared and dynamic properties only, and whole-object rows
/// stay alive in the result after the source array is gone.
#[test]
fn test_array_column_object_rows_read_public_properties() {
    assert_clean(
        r#"<?php
class User {
    public $id;
    public $name;
    protected $secret = "hidden";
    private $token = "private";
    public function __construct(int $id, string $name) { $this->id = $id; $this->name = $name; }
}
function run(int $i): string {
    $dynamic = new stdClass();
    $dynamic->id = "dyn" . $i;
    $dynamic->name = "Dyn";
    $rows = [new User(7, "Ada" . $i), $dynamic, ["id" => 9, "name" => "Arr"], new User(8, "Lin")];
    $names = array_column($rows, "name", "id");
    $hidden = array_column($rows, "secret");
    $private = array_column($rows, "token");
    $objects = array_column($rows, null, "id");
    return json_encode($names) . count($hidden) . count($private) . json_encode(array_keys($objects))
        . get_class($objects[7]) . $objects[8]->name;
}
$out = "";
for ($i = 0; $i < 25; $i++) { $out = run($i); }
echo $out;
"#,
        r#"{"7":"Ada24","dyn24":"Dyn","9":"Arr","8":"Lin"}00[7,"dyn24",9,8]UserLin"#,
    );
}

/// Array and object index values, and `mixed` key arguments holding an array or an object,
/// raise PHP's catchable TypeErrors after releasing the partial result.
#[test]
fn test_array_column_illegal_keys_raise_type_errors() {
    assert_clean(
        r#"<?php
class Key {}
function run(int $i): string {
    $out = "";
    try { array_column([["id" => [1], "n" => "s" . $i]], "n", "id"); } catch (TypeError $e) { $out .= $e->getMessage() . "|"; }
    try { array_column([["id" => new Key(), "n" => "s" . $i]], "n", "id"); } catch (TypeError $e) { $out .= $e->getMessage() . "|"; }
    $column = $i >= 0 ? new Key() : "n";
    try { array_column([["n" => "s" . $i]], $column); } catch (TypeError $e) { $out .= $e->getMessage() . "|"; }
    $index = $i >= 0 ? [1] : "n";
    try { array_column([["n" => "s" . $i]], "n", $index); } catch (TypeError $e) { $out .= $e->getMessage() . "|"; }
    return $out;
}
$out = "";
for ($i = 0; $i < 25; $i++) { $out = run($i); }
echo $out;
"#,
        "Cannot access offset of type array on array|Cannot access offset of type Key on array|\
array_column(): Argument #2 ($column_key) must be of type string|int|null, Key given|\
array_column(): Argument #3 ($index_key) must be of type string|int|null, array given|",
    );
}

/// Declared `array` parameters, named arguments and case-insensitive calls reach the same
/// walker; the boxed entry still rejects nothing that is an array.
#[test]
fn test_array_column_declared_array_named_and_case_insensitive() {
    assert_clean(
        r#"<?php
function index(array $rows, string $column, string $key): array {
    return ARRAY_COLUMN(index_key: $key, column_key: $column, array: $rows);
}
function run(int $i): string {
    $rows = [["sku" => "a" . $i, "qty" => 3], ["sku" => "b", "qty" => 5]];
    return json_encode(index($rows, "qty", "sku")) . json_encode(\array_column($rows, "sku"));
}
$out = "";
for ($i = 0; $i < 25; $i++) { $out = run($i); }
echo $out;
"#,
        r#"{"a24":3,"b":5}["a24","b"]"#,
    );
}
