//! Purpose:
//! Regression coverage for keyed predicates over arbitrary PHP array storage.
//!
//! Called from:
//! - The native codegen suite's array module.
//!
//! Key details:
//! - Heap and tagged fixtures cover snapshots, short-circuiting and exceptional ownership.
//! - Declared array parameters must preserve real element types and integer/string keys.

use crate::support::{compile_and_run_tagged, compile_and_run_with_heap_debug};

/// Asserts native output, a clean managed heap and equivalent tagged-null behavior.
fn assert_predicate_output(source: &str, expected: &str) {
    let output = compile_and_run_with_heap_debug(source);
    assert!(output.success, "stdout={:?}\nstderr={}", output.stdout, output.stderr);
    assert_eq!(output.stdout, expected, "{}", output.stderr);
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
    assert_eq!(compile_and_run_tagged(source), expected);
}

/// Declared arrays preserve string, false, float, null and nested-array values returned by find.
#[test]
fn test_array_predicates_preserve_boxed_value_types_and_source_keys() {
    assert_predicate_output(r#"<?php
function selectedPredicateType(array $values): void {
    $found = array_find($values, static fn(mixed $value, mixed $key): bool => $key === "wanted");
    echo gettype($found), "|";
    unset($found);
}
for ($i = 0; $i < 3; $i++) {
    selectedPredicateType(["skip" => 0, "wanted" => "text"]);
    selectedPredicateType(["skip" => 0, "wanted" => false]);
    selectedPredicateType(["skip" => 0, "wanted" => 1.25]);
    selectedPredicateType(["skip" => 0, "wanted" => null]);
    selectedPredicateType(["skip" => 0, "wanted" => [9, 8]]);
    $found = array_find([false, false], static fn(false $value): bool => true);
    echo gettype($found), "|";
    unset($found);
}
"#, &"string|boolean|double|NULL|array|boolean|".repeat(3));
}

/// Predicates see logical keys, coerce truthy results and stop before later callbacks run.
#[test]
fn test_array_predicates_keep_key_order_and_short_circuit() {
    assert_predicate_output(r#"<?php
function predicateKey(int $value, int $key): bool { return $key === 8; }
function predicateKeyName(bool $first): string { return $first ? "predicateKey" : "predicateKey"; }
for ($i = 0; $i < 3; $i++) {
    $values = [2 => 20, 4 => 40, 8 => 80];
    unset($values[4]);
    $calls = 0;
    $any = array_any($values, function(int $value, int $key) use (&$calls): string {
        $calls++; echo $key, ","; return "truthy";
    });
    echo (int) $any, ":", $calls, "|";
    $calls = 0;
    $all = array_all($values, function(int $value, int $key) use (&$calls): string {
        $calls++; echo $key, ","; return "0";
    });
    echo (int) $all, ":", $calls, "|";
    echo array_find($values, predicateKeyName($i === 0)), "|";
    echo (int) \ArRaY_AnY(callback: predicateKey(...), array: $values), "|";
    $calls = 0;
    $empty = static function(mixed $value, mixed $key) use (&$calls): bool { $calls++; return true; };
    echo (int) array_any([], $empty), (int) array_all([], $empty), ":", $calls, "|";
    unset($values, $empty);
}
"#, &"2,1:1|2,0:1|80|1|01:0|".repeat(3));
}

/// Callback writes detach from the retained traversal snapshot and find returns an independent owner.
#[test]
fn test_array_predicate_snapshot_survives_callback_replacement_and_nested_calls() {
    assert_predicate_output(r#"<?php
class PredicateOwnedResult {
    public string $value = "held";
    public function __destruct() { echo "released|"; }
}
for ($i = 0; $i < 3; $i++) {
    $values = ["a" => 1, "b" => 2, "c" => 3];
    $calls = 0;
    $all = array_all($values, function(int $value, string $key) use (&$values, &$calls): bool {
        $calls++; echo $key;
        $values = ["replacement" => 99];
        return array_any([false], static fn(false $inner): bool => !$inner);
    });
    echo ":", (int) $all, ":", $calls, ":", $values["replacement"], "|";
    $objects = [new PredicateOwnedResult()];
    $found = array_find($objects, static fn(PredicateOwnedResult $value): bool => true);
    unset($objects);
    echo $found->value, "|";
    unset($found, $values);
}
"#, &"abc:1:3:99|held|released|".repeat(3));
}

/// Throwing callbacks retire input, key, descriptor and argument owners before a later search succeeds.
#[test]
fn test_array_predicate_callback_exceptions_leave_clean_heap() {
    assert_predicate_output(r#"<?php
function failingArrayPredicate(string $value, string $key): bool {
    throw new RuntimeException("predicate failure");
}
for ($i = 0; $i < 3; $i++) {
    try { array_find(["key" => str_repeat("x", 24)], failingArrayPredicate(...)); }
    catch (RuntimeException $error) { echo $error->getMessage(), "|"; unset($error); }
    echo (int) array_all([false], static fn(false $value): bool => !$value), "|";
}
"#, &"predicate failure|1|".repeat(3));
}
