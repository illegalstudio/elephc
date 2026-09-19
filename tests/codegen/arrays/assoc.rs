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

// --- Issue #1087: array_push() onto an associative receiver -------------------------------
//
// PHP has one array type and one append: `array_push($hash, $v)` is `$hash[] = $v`, inserting at
// `max(int keys) + 1`, or `0` when the hash has no integer keys. AOT rejected every associative
// receiver outright with `array_push() first argument must be array`, while the Magician and the
// `array_unshift` contract both accepted one.
//
// The matrix below is the associative counterpart of
// `test_array_push_accepts_phps_full_variadic_signature`: it walks the value counts, the key
// forms a hash can already hold, every receiver place, and the return value. Each expected value
// is verbatim host PHP 8.5.10 output for the same fixture.

/// The full `array_push()` surface over an associative receiver.
///
/// Keys are read back with `foreach` rather than `array_keys()`: that builtin reads a hash key's
/// DECLARED type rather than its runtime form, so it dies on exactly the mixed int/string key
/// set an associative push produces (issue #1072). Using it here would assert that defect
/// instead of this one.
#[test]
fn test_array_push_appends_to_an_associative_receiver() {
    let out = compile_and_run(
        r#"<?php
$a = ["a" => 1, "b" => 2]; $n = array_push($a, 3);       echo implode(",", $a), "|", $n, "\n";
$b = ["a" => 1];           $n = array_push($b, 3, 4, 5); echo implode(",", $b), "|", $n, "\n";
$c = ["a" => 1];           $n = array_push($c);          echo implode(",", $c), "|", $n, "\n";
$d = [5 => "x", "k" => "y"];  array_push($d, "z");       foreach ($d as $k => $v) { echo $k, "="; } echo "\n";
$e = [-3 => "a", 7 => "b"];   array_push($e, "c");       foreach ($e as $k => $v) { echo $k, "="; } echo "\n";
function viaRef(array &$r): int { return array_push($r, "new"); }
$p = ["k" => "v"]; $n = viaRef($p); echo implode(",", $p), "|", $n, "\n";
class PushBox { public array $items = ["a" => 1]; }
$box = new PushBox(); $n = array_push($box->items, 2); echo implode(",", $box->items), "|", $n, "\n";
class PushShelf { public static array $items = ["a" => 1]; }
$n = array_push(PushShelf::$items, 2); echo implode(",", PushShelf::$items), "|", $n, "\n";
$rows = ["inner" => ["a" => 1]]; $n = array_push($rows["inner"], 2); echo implode(",", $rows["inner"]), "|", $n, "\n";
$g = ["a" => 1]; echo array_push($g, 2, 3) + 10, "\n";
$loop = ["k" => "v"]; for ($i = 0; $i < 5; $i++) { array_push($loop, (string) $i); }
foreach ($loop as $k => $v) { echo $k, "="; } echo "\n";
"#,
    );
    assert_eq!(
        out,
        concat!(
            "1,2,3|3\n",
            "1,3,4,5|4\n",
            "1|1\n",
            "5=k=6=\n",
            "-3=7=8=\n",
            "v,new|2\n",
            "1,2|2\n",
            "1,2|2\n",
            "1,2|2\n",
            "13\n",
            "k=0=1=2=3=4=\n",
        )
    );
}

/// The receiver is republished after EVERY insert, not once at the end.
///
/// A hash insert can split the table for copy-on-write, so the pointer a later insert must
/// address is the previous insert's result. Publishing once at the end wrote the pre-split
/// pointer back and the appends vanished — `count()` stayed at its original value while the
/// copy was, correctly, left alone. Both directions are asserted because only the pair
/// distinguishes a lost write from a split that never happened.
#[test]
fn test_array_push_on_a_shared_associative_receiver_separates_it() {
    let out = compile_and_run(
        r#"<?php
$h = ["a" => 1]; $b = $h; array_push($h, 2); echo count($h), ",", count($b), "\n";
$i = ["a" => 1]; $c = $i; array_push($c, 2); echo count($i), ",", count($c), "\n";
"#,
    );
    assert_eq!(out, "2,1\n1,2\n");
}

/// Appending to an associative receiver in a loop leaves no heap behind.
///
/// Each insert goes through `__rt_hash_set`, which owns the growth and the copy-on-write split,
/// and the caller republishes the table pointer afterwards. A missed release on the split-away
/// table accumulates per iteration rather than showing up once.
#[test]
fn test_array_push_on_an_associative_receiver_is_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
for ($i = 0; $i < 32; $i++) {
    $a = ["x" => 1, "y" => 2];
    array_push($a, 3, 4, 5);
    $b = ["x" => "aa"];
    $c = $b;
    array_push($b, "bb", "cc");
}
echo count($a), count($b), count($c), "\n";
"#,
    );
    assert_eq!(out.stdout, "531\n", "stderr: {}", out.stderr);
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "associative array_push leaked: {}",
        out.stderr
    );
}

/// Enough values to force `__rt_hash_grow`, which FREES the old table rather than merely
/// decrementing it.
///
/// A fresh one-entry literal is allocated with sixteen slots and grows once the count reaches
/// three quarters of that, so a five-value push never reaches the path at all: the earlier
/// fixtures exercise only the copy-on-write split. Growth is where a stale republished pointer
/// is a use-after-free instead of a stale read, so the shared receiver is included to make the
/// sequence split-then-grow rather than grow alone.
#[test]
fn test_array_push_growing_an_associative_receiver_is_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$a = ["a" => 1];
$n = array_push($a, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21);
$b = ["a" => 1];
$shared = $b;
$m = array_push($b, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21);
class GrowBox { public array $h = ["a" => 1]; }
$o = new GrowBox();
$p = array_push($o->h, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21);
echo $n, ",", $m, ",", count($shared), ",", $p, "\n";
"#,
    );
    assert_eq!(out.stdout, "21,21,1,21\n", "stderr: {}", out.stderr);
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "growing an associative receiver leaked: {}",
        out.stderr
    );
}

/// A receiver that is a MISSED hash read is the in-band null-container sentinel, not a table.
///
/// An `AssocArray`-typed value can hold that sentinel at run time, and dereferencing it
/// segfaulted — in the key scan when values were pushed, and in the count read when they were
/// not, since with no values no insert happens and there is nothing to have made the receiver
/// real. PHP answers with a `TypeError`, and so does this now, with php-src's wording.
#[test]
fn test_array_push_on_a_missed_associative_read_raises_a_type_error() {
    let out = compile_and_run(
        r#"<?php
$h = ["a" => ["x" => 1]];
try { array_push($h["missing"], 2); } catch (TypeError $e) { echo "with:", $e->getMessage(), "\n"; }
try { array_push($h["absent"]); } catch (TypeError $e) { echo "without:", $e->getMessage(), "\n"; }
"#,
    );
    assert_eq!(
        out,
        concat!(
            "with:array_push(): Argument #1 ($array) must be of type array, null given\n",
            "without:array_push(): Argument #1 ($array) must be of type array, null given\n",
        )
    );
}
