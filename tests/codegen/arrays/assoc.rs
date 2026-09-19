//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of array suites, including array basic, array integer values, and array assign.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use crate::support::*;

// --- Phase 12: v0.6 — Associative arrays, switch, match ---

/// Compiles a PHP script with two static string-keyed entries and verifies the first value is echoed.
#[test]
fn test_assoc_array_basic() {
    let out = compile_and_run(
        r#"<?php
$m = ["name" => "Alice", "city" => "NYC"];
echo $m["name"];
"#,
    );
    assert_eq!(out, "Alice");
}

/// Compiles a PHP script with three static string keys mapping to integer values and verifies addition.
#[test]
fn test_assoc_array_int_values() {
    let out = compile_and_run(
        r#"<?php
$m = ["a" => 1, "b" => 2, "c" => 3];
echo $m["a"] + $m["b"] + $m["c"];
"#,
    );
    assert_eq!(out, "6");
}

/// Verifies an associative literal stores the declared object result of a method call instead of
/// stamping the hash value layout from the syntactic integer fallback.
#[test]
fn test_assoc_array_literal_property_receiver_method_element_typed_by_return() {
    let out = compile_and_run(
        r#"<?php
declare(strict_types=1);
final class Link { public function __construct(public string $label) {} }
final class Factory { public function link(string $label): Link { return new Link($label); } }
final class Composer {
    public function __construct(private Factory $factory) {}
    public function links(): array { return ['view' => $this->factory->link('View')]; }
}
$links = (new Composer(new Factory()))->links();
echo $links['view']->label;
"#,
    );
    assert_eq!(out, "View");
}

/// Compiles a PHP script that creates an assoc array with one entry then appends a second key and verifies both values are summed.
#[test]
fn test_assoc_array_assign() {
    let out = compile_and_run(
        r#"<?php
$m = ["x" => 10];
$m["y"] = 20;
echo $m["x"] + $m["y"];
"#,
    );
    assert_eq!(out, "30");
}

/// Compiles a PHP script that creates an assoc array, overwrites its sole key, and verifies the new value is returned.
#[test]
fn test_assoc_array_update() {
    let out = compile_and_run(
        r#"<?php
$m = ["key" => "old"];
$m["key"] = "new";
echo $m["key"];
"#,
    );
    assert_eq!(out, "new");
}

/// Compiles a PHP script that populates an assoc array using a loop with dynamic concatenated keys ("k" . $i) and verifies count and boundary access.
#[test]
fn test_assoc_array_dynamic_string_key_assignment_loop_counts() {
    let out = compile_and_run(
        r#"<?php
$a = [];
for ($i = 0; $i < 5; $i++) {
    $a["k" . $i] = $i;
}
echo count($a) . ":" . $a["k0"] . ":" . $a["k4"];
"#,
    );
    assert_eq!(out, "5:0:4");
}

/// Compiles a PHP script that assigns both string and integer values to an assoc array and verifies both scalar payloads are preserved and concatenated.
#[test]
fn test_assoc_array_mixed_assignment_access_preserves_scalar_payloads() {
    let out = compile_and_run(
        r#"<?php
$a = [];
$a["Host"] = "example.com";
$a["Port"] = 80;
echo $a["Host"] . "|" . $a["Port"];
"#,
    );
    assert_eq!(out, "example.com|80");
}

/// Compiles a PHP script with a function that builds an assoc array using dynamic string keys in a loop, returns it, and verifies count and boundary access from the caller.
#[test]
fn test_assoc_array_dynamic_string_key_assignment_inside_function() {
    let out = compile_and_run(
        r#"<?php
function build_map(int $n): array {
    $a = [];
    for ($i = 0; $i < $n; $i++) {
        $a["k" . $i] = $i;
    }
    return $a;
}
$a = build_map(5);
echo count($a) . ":" . $a["k0"] . ":" . $a["k4"];
"#,
    );
    assert_eq!(out, "5:0:4");
}

/// Compiles a PHP script that initializes an indexed array then adds a dynamic string-keyed entry alongside numeric indices, verifying the integer keys are preserved.
#[test]
fn test_assoc_array_dynamic_string_key_after_indexed_literal_preserves_int_keys() {
    let out = compile_and_run(
        r#"<?php
$a = [10, 20];
$a["k" . 2] = 30;
echo count($a) . ":" . $a[0] . ":" . $a[1] . ":" . $a["k2"];
"#,
    );
    assert_eq!(out, "3:10:20:30");
}

/// Compiles a PHP script with assoc keys using integer, numeric string, and leading-zero string forms and verifies PHP's key normalization and lookup behavior.
#[test]
fn test_assoc_array_integer_and_numeric_string_keys() {
    let out = compile_and_run(
        r#"<?php
$m = [1 => "one", "2" => "two", "01" => "leading"];
echo $m[1] . "|" . $m["1"] . "|" . $m[2] . "|" . $m["01"];
"#,
    );
    assert_eq!(out, "one|one|two|leading");
}

/// Compiles a PHP script exercising boundary cases for numeric string keys: "0", "00", "-1", "-0", PHP_INT_MAX, PHP_INT_MAX+1, PHP_INT_MIN, PHP_INT_MIN-1. Verifies key normalization and overflow/underflow behavior.
#[test]
fn test_assoc_array_numeric_string_key_boundaries() {
    let out = compile_and_run(
        r#"<?php
$m = [
    "0" => "zero",
    "00" => "double-zero",
    "-1" => "negative",
    "-0" => "negative-zero",
    "9223372036854775807" => "max",
    "9223372036854775808" => "overflow",
    "-9223372036854775808" => "min",
    "-9223372036854775809" => "underflow",
];
echo $m[0] . "|" . $m["00"] . "|" . $m[-1] . "|" . $m["-0"] . "|";
echo $m[PHP_INT_MAX] . "|" . $m["9223372036854775808"] . "|";
echo $m[PHP_INT_MIN] . "|" . $m["-9223372036854775809"];
"#,
    );
    assert_eq!(
        out,
        "zero|double-zero|negative|negative-zero|max|overflow|min|underflow"
    );
}

/// Compiles a PHP script that assigns via integer key, then updates via numeric string keys ("1", "01") and verifies count and lookup behavior reflects PHP's key normalization.
#[test]
fn test_assoc_array_numeric_string_assignment_updates_integer_key() {
    let out = compile_and_run(
        r#"<?php
$m = [1 => "left"];
$m["1"] = "right";
$m["01"] = "leading";
echo count($m) . ":" . $m[1] . ":" . $m["01"];
"#,
    );
    assert_eq!(out, "2:right:leading");
}

/// Compiles a PHP script that sparsely populates an array with non-contiguous integer keys and verifies iteration produces only the assigned integer keys.
#[test]
fn test_sparse_integer_key_assignment_uses_php_array_keys() {
    let out = compile_and_run(
        r#"<?php
$a = [];
$a[3] = "x";
$a[5] = "y";
echo count($a) . "|";
foreach ($a as $k => $v) {
    echo $k . "=" . $v . ";";
}
"#,
    );
    assert_eq!(out, "2|3=x;5=y;");
}

/// Compiles a PHP script that performs array union (+) with two assoc arrays sharing a key "a" and verifies the left operand's value is retained for the duplicate key.
#[test]
fn test_assoc_array_union_keeps_left_duplicate_keys() {
    let out = compile_and_run(
        r#"<?php
$left = ["a" => "left", "b" => "keep"];
$right = ["a" => "right", "c" => "new"];
$result = $left + $right;
echo count($result) . ":";
foreach ($result as $k => $v) {
    echo $k . "=" . $v . " ";
}
"#,
    );
    assert_eq!(out, "3:a=left b=keep c=new ");
}

/// Compiles a PHP script that performs array union with integer key on the left and numeric string key on the right that refer to the same PHP key, and verifies normalization keeps left value.
#[test]
fn test_assoc_array_union_normalizes_numeric_string_duplicates() {
    let out = compile_and_run(
        r#"<?php
$left = [1 => "left"];
$right = ["1" => "right", 2 => "new"];
$result = $left + $right;
echo count($result) . ":" . $result[1] . ":" . $result[2];
"#,
    );
    assert_eq!(out, "2:left:new");
}

/// Compiles a PHP script that performs array union with string-keyed assoc arrays containing integer values and verifies the sum of retained values.
#[test]
fn test_assoc_array_union_int_values() {
    let out = compile_and_run(
        r#"<?php
$left = ["a" => 1, "b" => 2];
$right = ["b" => 99, "c" => 3];
$result = $left + $right;
echo $result["a"] + $result["b"] + $result["c"];
"#,
    );
    assert_eq!(out, "6");
}

/// Compiles a PHP script that performs array union where both operands are assoc builtins (array_fill_keys, array_combine) and verifies the sum of retained values.
#[test]
fn test_assoc_array_union_with_assoc_builtin_operands() {
    let out = compile_and_run(
        r#"<?php
$left = array_fill_keys(["a", "b"], 1);
$right = array_combine(["b", "c"], [99, 3]);
$result = $left + $right;
echo $result["a"] + $result["b"] + $result["c"];
"#,
    );
    assert_eq!(out, "5");
}

/// Compiles a PHP script that performs array union where the left operand is the result of array_diff_key and verifies the sum of retained values.
#[test]
fn test_assoc_array_union_with_key_filter_builtin_operand() {
    let out = compile_and_run(
        r#"<?php
$left = array_diff_key(["a" => 1, "b" => 2], ["a" => 0]);
$right = ["b" => 99, "c" => 3];
$result = $left + $right;
echo $result["b"] + $result["c"];
"#,
    );
    assert_eq!(out, "5");
}

/// Compiles a PHP script that performs array union with a left indexed array and right assoc array sharing integer and string keys, verifying the shared key space produces the expected count and iteration order.
#[test]
fn test_indexed_plus_assoc_array_union_uses_shared_key_space() {
    let out = compile_and_run(
        r#"<?php
$left = ["zero", "one"];
$right = [0 => "skip-zero", "1" => "skip-one", "01" => "leading", 2 => "two", "name" => "alice"];
$result = $left + $right;
echo count($result) . ":";
foreach ($result as $k => $v) {
    echo $k . "=" . $v . ";";
}
echo "|" . $result[0] . "|" . $result[1] . "|" . $result["01"] . "|" . $result[2] . "|" . $result["name"];
"#,
    );
    assert_eq!(
        out,
        "5:0=zero;1=one;01=leading;2=two;name=alice;|zero|one|leading|two|alice"
    );
}

/// Compiles a PHP script that performs array union with a left assoc array (string and "0"/"01" keys) and a right indexed array, verifying the shared key space produces the expected count and iteration order.
#[test]
fn test_assoc_plus_indexed_array_union_uses_shared_key_space() {
    let out = compile_and_run(
        r#"<?php
$left = ["0" => "zero-left", "01" => "leading-left", "name" => "left"];
$right = ["zero-right", "one-right", "two-right"];
$result = $left + $right;
echo count($result) . ":";
foreach ($result as $k => $v) {
    echo $k . "=" . $v . ";";
}
echo "|" . $result[0] . "|" . $result[1] . "|" . $result["01"] . "|" . $result[2];
"#,
    );
    assert_eq!(
        out,
        "5:0=zero-left;01=leading-left;name=left;1=one-right;2=two-right;|zero-left|one-right|leading-left|two-right"
    );
}

/// Compiles a PHP script that performs array union with mixed indexed and assoc arrays containing nested arrays, then unsets the operands and verifies the result retains nested values.
#[test]
fn test_mixed_representation_array_union_retains_nested_values() {
    let out = compile_and_run(
        r#"<?php
$left = [[10], [20]];
$right = ["meta" => [30], 0 => [99]];
$result = $left + $right;
unset($left);
unset($right);
echo $result[0][0] . "|" . $result[1][0] . "|" . $result["meta"][0];
"#,
    );
    assert_eq!(out, "10|20|30");
}

/// Compiles a PHP script that performs array union inside a function with indexed left and assoc right, then iterates via foreach, verifying key/value preservation.
#[test]
fn test_indexed_plus_assoc_array_union_inside_function_foreach() {
    let out = compile_and_run(
        r#"<?php
function render(): void {
    $left = ["zero", "one"];
    $right = [0 => "skip", "name" => "alice"];
    $result = $left + $right;
    foreach ($result as $k => $v) {
        echo $k . "=" . $v . ";";
    }
}
render();
"#,
    );
    assert_eq!(out, "0=zero;1=one;name=alice;");
}

/// Compiles a PHP script that performs array union inside a function with assoc left and indexed right, then iterates via foreach, verifying key/value preservation.
#[test]
fn test_assoc_plus_indexed_array_union_inside_function_foreach() {
    let out = compile_and_run(
        r#"<?php
function render(): void {
    $left = ["0" => "zero-left", "name" => "left"];
    $right = ["zero-right", "one-right"];
    $result = $left + $right;
    foreach ($result as $k => $v) {
        echo $k . "=" . $v . ";";
    }
}
render();
"#,
    );
    assert_eq!(out, "0=zero-left;name=left;1=one-right;");
}

/// Compiles a PHP script that iterates an assoc array using key=>value foreach syntax and verifies both key and value are emitted correctly.
#[test]
fn test_assoc_foreach_key_value() {
    let out = compile_and_run(
        r#"<?php
$m = ["a" => "1", "b" => "2"];
foreach ($m as $k => $v) {
    echo $k . "=" . $v . " ";
}
"#,
    );
    assert_eq!(out, "a=1 b=2 ");
}

/// Compiles a PHP script that iterates an assoc array by reference, mutating each value in-place, then iterates again by value to verify mutations were applied.
#[test]
fn test_assoc_foreach_value_by_reference_mutates_values() {
    let out = compile_and_run(
        r#"<?php
$m = ["a" => 1, "b" => 2];
foreach ($m as $k => &$v) {
    $v += 10;
}
foreach ($m as $k => $x) {
    echo $k . "=" . $x . ";";
}
"#,
    );
    assert_eq!(out, "a=11;b=12;");
}

/// Compiles a PHP script that iterates an assoc array by reference, then reuses the same variable name `$v` in a second loop by value, verifying the reference loop's effect is not carried over.
#[test]
fn test_assoc_foreach_value_by_reference_reuse_value_name_in_next_loop() {
    let out = compile_and_run(
        r#"<?php
$m = ["a" => 1, "b" => 2];
foreach ($m as $k => &$v) {
    $v += 10;
}
foreach ($m as $k => $v) {
    echo $k . "=" . $v . ";";
}
"#,
    );
    assert_eq!(out, "a=11;b=11;");
}

/// Compiles a PHP script that iterates an assoc array by reference, then assigns to the reference variable after the loop and verifies the last element is mutated while the reference variable holds the assigned value.
#[test]
fn test_assoc_foreach_value_by_reference_post_assignment_mutates_last_element() {
    let out = compile_and_run(
        r#"<?php
$m = ["a" => 1, "b" => 2];
foreach ($m as &$v) {
    $v += 10;
}
$v = 99;
foreach ($m as $k => $x) {
    echo $k . "=" . $x . ";";
}
echo "|" . $v;
"#,
    );
    assert_eq!(out, "a=11;b=99;|99");
}

/// Compiles a PHP script that iterates an assoc array with mixed integer and string keys (including leading zeros) and verifies the key type is preserved as written.
#[test]
fn test_assoc_foreach_mixed_integer_and_string_keys() {
    let out = compile_and_run(
        r#"<?php
$m = [1 => "a", "02" => "b"];
foreach ($m as $k => $v) {
    echo $k . "=" . $v . ";";
}
"#,
    );
    assert_eq!(out, "1=a;02=b;");
}

/// Compiles a PHP script that overwrites an existing key in an assoc array and verifies iteration order is preserved (original key order maintained with updated value).
#[test]
fn test_assoc_foreach_preserves_order_after_overwrite() {
    let out = compile_and_run(
        r#"<?php
$m = ["a" => "1", "b" => "2"];
$m["a"] = "3";
foreach ($m as $k => $v) {
    echo $k . "=" . $v . " ";
}
"#,
    );
    assert_eq!(out, "a=3 b=2 ");
}

/// Compiles a PHP script that grows an assoc array from 1 to 13 entries and verifies iteration order is preserved across growth.
#[test]
fn test_assoc_foreach_preserves_order_after_growth() {
    let out = compile_and_run(
        r#"<?php
$m = ["k0" => "0"];
$m["k1"] = "1";
$m["k2"] = "2";
$m["k3"] = "3";
$m["k4"] = "4";
$m["k5"] = "5";
$m["k6"] = "6";
$m["k7"] = "7";
$m["k8"] = "8";
$m["k9"] = "9";
$m["k10"] = "10";
$m["k11"] = "11";
$m["k12"] = "12";
foreach ($m as $k => $v) {
    echo $k . "=" . $v . " ";
}
"#,
    );
    assert_eq!(
        out,
        "k0=0 k1=1 k2=2 k3=3 k4=4 k5=5 k6=6 k7=7 k8=8 k9=9 k10=10 k11=11 k12=12 "
    );
}

/// Compiles a PHP script that iterates a plain indexed array using key=>value foreach syntax and verifies the auto-incremented integer keys are emitted correctly.
#[test]
fn test_indexed_foreach_key_value() {
    let out = compile_and_run(
        r#"<?php
$arr = [10, 20, 30];
foreach ($arr as $i => $v) {
    echo $i . ":" . $v . " ";
}
"#,
    );
    assert_eq!(out, "0:10 1:20 2:30 ");
}

/// Compiles a PHP switch statement with integer case values and verifies the correct branch is selected and "two" is echoed.
#[test]
fn test_switch_basic() {
    let out = compile_and_run(
        r#"<?php
$x = 2;
switch ($x) {
    case 1:
        echo "one";
        break;
    case 2:
        echo "two";
        break;
    case 3:
        echo "three";
        break;
}
"#,
    );
    assert_eq!(out, "two");
}

/// Compiles a PHP switch statement with a default branch and verifies "other" is echoed when no case matches.
#[test]
fn test_switch_default() {
    let out = compile_and_run(
        r#"<?php
$x = 99;
switch ($x) {
    case 1:
        echo "one";
        break;
    default:
        echo "other";
        break;
}
"#,
    );
    assert_eq!(out, "other");
}

/// Compiles a PHP switch statement with a fallthrough case (case 1 falls through to case 2) and verifies both branches execute producing "ab".
#[test]
fn test_switch_fallthrough() {
    let out = compile_and_run(
        r#"<?php
$x = 1;
switch ($x) {
    case 1:
        echo "a";
    case 2:
        echo "b";
        break;
    case 3:
        echo "c";
        break;
}
"#,
    );
    assert_eq!(out, "ab");
}

/// Compiles a PHP switch statement with string case values and verifies the correct branch is selected.
#[test]
fn test_switch_string() {
    let out = compile_and_run(
        r#"<?php
$s = "hello";
switch ($s) {
    case "hi":
        echo "A";
        break;
    case "hello":
        echo "B";
        break;
    default:
        echo "C";
        break;
}
"#,
    );
    assert_eq!(out, "B");
}

/// Compiles a PHP match expression with integer arms and verifies the correct arm is returned.
#[test]
fn test_match_basic() {
    let out = compile_and_run(
        r#"<?php
$x = 2;
$result = match($x) {
    1 => "one",
    2 => "two",
    3 => "three",
    default => "other",
};
echo $result;
"#,
    );
    assert_eq!(out, "two");
}

/// Compiles a PHP match expression where no arm matches and verifies the default arm is returned.
#[test]
fn test_match_default() {
    let out = compile_and_run(
        r#"<?php
$x = 99;
echo match($x) {
    1 => "one",
    default => "unknown",
};
"#,
    );
    assert_eq!(out, "unknown");
}

/// Compiles a standalone match expression statement and verifies following statements still run.
#[test]
fn test_standalone_match_expression_statement() {
    let out = compile_and_run(
        r#"<?php
match (1) {
    1 => 2,
};
echo 3;
"#,
    );
    assert_eq!(out, "3");
}

/// Regression for #357: a null array key after a float key normalizes to the empty
/// string "" (PHP behavior), not a huge integer sentinel.
#[test]
fn test_null_key_after_float_key() {
    let out = compile_and_run(
        r#"<?php
$a = [];
$a[1.9] = 1;
$a[null] = 2;
foreach ($a as $k => $v) { echo '[' . $k . ':' . $v . ']'; }
"#,
    );
    assert_eq!(out, "[1:1][:2]");
}

/// Regression for #357: a null array key before a float key normalizes to the empty
/// string "" (PHP behavior), preserving insertion order.
#[test]
fn test_null_key_before_float_key() {
    let out = compile_and_run(
        r#"<?php
$a = [];
$a[null] = 1;
$a[1.9] = 2;
foreach ($a as $k => $v) { echo '[' . $k . ':' . $v . ']'; }
"#,
    );
    assert_eq!(out, "[:1][1:2]");
}

/// Regression for #357: a null key in an array literal normalizes to the empty string.
#[test]
fn test_null_key_in_array_literal() {
    let out = compile_and_run(
        r#"<?php
$a = [null => "first", "x" => "second", 5 => "third"];
foreach ($a as $k => $v) { echo '[' . $k . ':' . $v . ']'; }
"#,
    );
    assert_eq!(out, "[:first][x:second][5:third]");
}

/// Regression for #357: a null foreach key rebuilt into a destination array keeps the
/// empty-string key instead of collapsing to a huge integer sentinel.
#[test]
fn test_null_foreach_key_rebuild() {
    let out = compile_and_run(
        r#"<?php
$src = [null => 1, "x" => 2];
$dst = [];
foreach ($src as $k => $v) { $dst[$k] = $v; }
foreach ($dst as $k => $v) { echo '[' . $k . ':' . $v . ']'; }
"#,
    );
    assert_eq!(out, "[:1][x:2]");
}

/// Regression for #357: array_key_exists with a null key matches the empty-string slot.
#[test]
fn test_array_key_exists_null_key() {
    let out = compile_and_run(
        r#"<?php
$a = ["" => "empty"];
echo isset($a[null]) ? "set" : "unset";
echo "|";
echo array_key_exists(null, $a) ? "yes" : "no";
"#,
    );
    assert_eq!(out, "set|yes");
}

/// Regression for #357: a null key read on an associative array misses unless the empty
/// string slot exists, matching PHP's null-to-empty-string normalization.
#[test]
fn test_null_key_read_miss() {
    let out = compile_and_run(
        r#"<?php
$a = ["x" => 1, "y" => 2];
echo $a[null] ?? "miss";
"#,
    );
    assert_eq!(out, "miss");
}


/// Verifies `array_slice()` on an ASSOCIATIVE receiver, in both `preserve_keys` modes
/// (issue #683).
///
/// Every associative receiver was refused at compile time — string-keyed and integer-keyed
/// alike, with and without the flag — behind two different messages: `array_slice preserve_keys
/// for PHP type AssocArray {…}` and `array_slice for PHP type AssocArray {…}`. The indexed
/// receiver was fully supported in both modes, so the gap was the receiver being a hash, not the
/// element type.
///
/// `preserve_keys` is NOT "keep all keys" versus "drop all keys", which is the rule the fixture
/// exists to pin: php-src only ever renumbers INTEGER keys, and a string key survives either
/// way. The third and fourth rows show an integer-keyed source renumbering, and the fifth and
/// sixth show a MIXED source where only the integer entries move.
///
/// `$offset` and `$length` count positions in insertion order rather than keys, so the rest of
/// the matrix walks the window arithmetic: an omitted length, a negative offset, a negative
/// length, an offset past the end, an offset before the start, a zero length, an over-long
/// length, and an empty source. Then the value kinds the copy has to own correctly — strings,
/// floats, bools, nested arrays — and a check that the source is untouched.
///
/// Every expected value is verbatim host PHP 8.5.10 output for the same fixture.
#[test]
fn test_array_slice_on_an_associative_receiver_matches_php() {
    let out = compile_and_run(
        r#"<?php
function show(array $a): void {
    $parts = [];
    foreach ($a as $k => $v) { $parts[] = var_export($k, true) . "=>" . var_export($v, true); }
    echo "[", implode(", ", $parts), "]\n";
}
show(array_slice(["x" => 1, "y" => 2, "z" => 3], 1, 2));
show(array_slice(["x" => 1, "y" => 2, "z" => 3], 1, 2, true));
show(array_slice([5 => 1, 9 => 2, 12 => 3], 1, 2));
show(array_slice([5 => 1, 9 => 2, 12 => 3], 1, 2, true));
show(array_slice([5 => "a", "k" => "b", 9 => "c"], 0, 3));
show(array_slice([5 => "a", "k" => "b", 9 => "c"], 0, 3, true));
show(array_slice(["a" => 1, "b" => 2, "c" => 3], 1));
show(array_slice(["a" => 1, "b" => 2, "c" => 3], 1, null, true));
show(array_slice(["a" => 1, "b" => 2, "c" => 3], -2));
show(array_slice(["a" => 1, "b" => 2, "c" => 3], -2, 1, true));
show(array_slice(["a" => 1, "b" => 2, "c" => 3, "d" => 4], 1, -1));
show(array_slice(["a" => 1, "b" => 2], 10, 2));
show(array_slice(["a" => 1, "b" => 2], -10, 1));
show(array_slice(["a" => 1, "b" => 2], 0, 0));
show(array_slice(["a" => 1, "b" => 2], 1, 100));
show(array_slice([], 0, 1));
show(array_slice(["x" => "aa", "y" => "bb", "z" => "cc"], 1, 2));
show(array_slice(["x" => "aa", "y" => "bb", "z" => "cc"], 1, 2, true));
show(array_slice(["x" => 1.5, "y" => 2.5], 1, 1));
show(array_slice(["x" => true, "y" => false], 0, 2));
$nested = array_slice(["x" => [1, 2], "y" => [3, 4]], 1, 1);
echo count($nested), ",", $nested["y"][0], ",", $nested["y"][1], "\n";
$src = ["a" => 1, "b" => 2, "c" => 3];
$cut = array_slice($src, 1, 1);
echo count($src), ",", count($cut), "\n";
show(array_slice([10, 20, 30], 1, 2));
show(array_slice([10, 20, 30], 1, 2, true));
"#,
    );
    assert_eq!(
        out,
        concat!(
            "['y'=>2, 'z'=>3]\n",
            "['y'=>2, 'z'=>3]\n",
            "[0=>2, 1=>3]\n",
            "[9=>2, 12=>3]\n",
            "[0=>'a', 'k'=>'b', 1=>'c']\n",
            "[5=>'a', 'k'=>'b', 9=>'c']\n",
            "['b'=>2, 'c'=>3]\n",
            "['b'=>2, 'c'=>3]\n",
            "['b'=>2, 'c'=>3]\n",
            "['b'=>2]\n",
            "['b'=>2, 'c'=>3]\n",
            "[]\n",
            "['a'=>1]\n",
            "[]\n",
            "['b'=>2]\n",
            "[]\n",
            "['y'=>'bb', 'z'=>'cc']\n",
            "['y'=>'bb', 'z'=>'cc']\n",
            "['y'=>2.5]\n",
            "['x'=>true, 'y'=>false]\n",
            "1,3,4\n",
            "3,1\n",
            "[0=>20, 1=>30]\n",
            "[1=>20, 2=>30]\n",
        )
    );
}

/// Verifies the associative `array_slice()` copy owns what it holds, and nothing more.
///
/// `__rt_hash_slice` retains the string keys and the refcounted values it carries over, and
/// re-persists string values, exactly as `__rt_hash_clone_shallow` does. A missing retain frees
/// storage the source still references; a missing release accumulates per call. The loop makes
/// either visible instead of hiding it in a single-iteration total.
#[test]
fn test_array_slice_on_an_associative_receiver_is_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
for ($i = 0; $i < 32; $i++) {
    $a = array_slice(["x" => 1, "y" => 2, "z" => 3], 1, 2);
    $b = array_slice(["x" => 1, "y" => 2, "z" => 3], 1, 2, true);
    $c = array_slice(["x" => "aa", "y" => "bb", "z" => "cc"], 1, 2);
    $d = array_slice(["x" => [1, 2], "y" => [3, 4]], 1, 1);
    $e = array_slice([5 => 1, 9 => 2, 12 => 3], 1, 2);
}
echo count($a), count($b), count($c), count($d), count($e), "\n";
"#,
    );
    assert_eq!(out.stdout, "22212\n", "stderr: {}", out.stderr);
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "associative array_slice leaked: {}",
        out.stderr
    );
}

// --- Issue #1049: a spread beside an explicit key ---

/// Verifies every order of spread and explicit key keeps BOTH, in PHP's order.
///
/// A literal that mixes the two fits neither single-shape AST node -- `ArrayLiteral` has no
/// place for a key, `ArrayLiteralAssoc` no place for a keyless spread -- and the parser resolved
/// that by dropping whichever did not fit. All five mixed shapes lost entries silently: no
/// warning, no error, just a shorter array (issue #1049).
///
/// The ORDER is the point. A spread's elements take the next free INTEGER key at the position
/// the spread occupies, so the same parts in a different order produce different keys, and a fix
/// that appended the spread at one end would still pass a single-order test.
///
/// Each case gets its OWN source array on purpose. Spreading one indexed array and then reading
/// it again is a separate, pre-existing defect -- the promotion corrupts the source's length --
/// and sharing a source here would measure that instead of what this fixture is for.
#[test]
fn test_spread_beside_an_explicit_key_keeps_both() {
    let out = compile_and_run(
        r#"<?php
$i1 = [3, 4];
$i2 = [3, 4];
$i3 = [3, 4];
$i4 = [3, 4];
$i5 = [3, 4];
$as = ["x" => 1, "y" => 2];

$c1 = [...$i1, "c" => 8];
$c2 = ["c" => 8, ...$i2];
$c3 = [...$as, "c" => 8];
$c4 = [...$i3, 7 => 8];
$c5 = ["a" => 1, ...$i4, "b" => 2];
$c6 = [...$i5, 5];

foreach ([$c1, $c2, $c3, $c4, $c5, $c6] as $case) {
    foreach ($case as $k => $v) { echo $k, "=", $v, ","; }
    echo "|";
}
"#,
    );
    assert_eq!(
        out,
        "0=3,1=4,c=8,|c=8,0=3,1=4,|x=1,y=2,c=8,|0=3,1=4,7=8,|a=1,0=3,1=4,b=2,|0=3,1=4,2=5,|"
    );
}

/// Verifies a mixed literal evaluates each part once, in source order.
///
/// Every entry is lowered where it appears, so a side-effecting spread source or value has to
/// run exactly once and in the written order. A fix that re-read an entry in order to give it a
/// key would show up here as a repeated letter, and one that hoisted the spread would reorder
/// them.
#[test]
fn test_a_mixed_literal_evaluates_each_entry_once_in_order() {
    let out = compile_and_run(
        r#"<?php
function t(string $tag, int $value): int { echo $tag; return $value; }
function ta(string $tag, array $value): array { echo $tag; return $value; }
$a = ["k" => t("a", 1), ...ta("b", [7, 8]), t("c", 9), "j" => t("d", 2)];
echo "|";
foreach ($a as $k => $v) { echo $k, "=", $v, ","; }
"#,
    );
    assert_eq!(out, "abcd|k=1,0=7,1=8,2=9,j=2,");
}

/// Verifies the tree walkers reach INSIDE a mixed literal's entries.
///
/// The new node is not a compile error for every pass: the walkers that end in a catch-all --
/// name resolution, autoload reference collection, include resolution, the loop-storage and
/// array-pointer scans -- would return it unchanged with its children unvisited. That failure is
/// silent, because a class name inside the literal simply never gets resolved. This fixture puts
/// a namespaced constant and a `new` where only those walkers can reach them.
#[test]
fn test_name_resolution_reaches_inside_a_mixed_literal() {
    let out = compile_and_run(
        r#"<?php
namespace App;

class Config {
    const HOST = "localhost";
    public int $port = 0;
}

function build(array $extra): array {
    return [...$extra, "host" => Config::HOST, "obj" => new Config()];
}

$out = build([1, 2]);
echo implode(",", array_keys($out)), "|", $out["host"], "|", get_class($out["obj"]);
"#,
    );
    assert_eq!(out, "0,1,host,obj|localhost|App\\Config");
}

/// Verifies the automatic integer key is the RUNTIME's to assign, not the parser's.
///
/// An explicit integer key moves the cursor for everything after it, and a spread consumes
/// however many slots its source turns out to have. Neither is a compile-time fact once a
/// spread is in the literal, so the lowering must leave both to the runtime -- a parser that
/// numbered the entries itself would get every one of these wrong.
///
/// Covers a high key before a spread, a negative key (PHP 8.3 continues from it), a key between
/// two spreads, an empty spread, and a string-key collision where the later operand wins.
#[test]
fn test_a_mixed_literal_leaves_automatic_keys_to_the_runtime() {
    let out = compile_and_run(
        r#"<?php
$i = [3, 4];

function show(string $label, array $case): void {
    echo $label, ":";
    foreach ($case as $k => $v) { echo $k, "=", $v, ","; }
    echo "|";
}

show("a", [...$i, 7 => 8, 9]);
show("b", [...$i, 9, 7 => 8]);
show("c", [20 => 1, ...$i]);
show("d", [-5 => 1, ...$i]);
show("e", [...$i, "m" => 0, ...$i]);
show("f", [...[], "only" => 1]);
show("g", ["c" => 1, ...["c" => 2]]);
"#,
    );
    assert_eq!(
        out,
        "a:0=3,1=4,7=8,8=9,|b:0=3,1=4,2=9,7=8,|c:20=1,21=3,22=4,|d:-5=1,-4=3,-3=4,|e:0=3,1=4,m=0,2=3,3=4,|f:only=1,|g:c=2,|"
    );
}

/// Verifies `yield from` accepts a mixed literal, as PHP does.
///
/// The `yield from` operand list is an ALLOWLIST, so the failure mode of a node missing from it
/// is the opposite of a walker's: valid PHP is refused rather than silently under-analysed. It
/// named the two older literal nodes, so `yield from [...$items, "k" => 1]` was rejected with
/// "expects an array literal or Generator" even though the lowering handles the resulting
/// `AssocArray` exactly as it handles a keyed literal.
#[test]
fn test_yield_from_accepts_a_mixed_literal() {
    let out = compile_and_run(
        r#"<?php
function g(array $items) {
    yield from [...$items, "k" => 1];
}
foreach (g([7, 8]) as $k => $v) { echo $k, "=", $v, ","; }
"#,
    );
    assert_eq!(out, "0=7,1=8,k=1,");
}
