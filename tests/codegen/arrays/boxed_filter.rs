//! Purpose:
//! Regression coverage for array filtering with boxed PHP values and preserved keys.
//!
//! Called from:
//! - The native codegen suite's array module.
//!
//! Key details:
//! - Heap/tagged fixtures cover result ownership, nullable callbacks and exceptions.
//! - Explicit profiles distinguish PHP 8.5 mode fallback from PHP 8.6 validation.

use crate::support::{compile_and_run_tagged, compile_and_run_with_heap_debug, compile_and_run_with_php_version};
use elephc::php_version::PhpVersion;

/// Asserts native output, complete managed-heap cleanup and tagged-null equivalence.
fn assert_filter_output(source: &str, expected: &str) {
    let output = compile_and_run_with_heap_debug(source);
    assert!(output.success, "stdout={:?}\nstderr={}", output.stdout, output.stderr);
    assert_eq!(output.stdout, expected, "{}", output.stderr);
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
    assert_eq!(compile_and_run_tagged(source), expected);
}

/// Null and omitted callbacks remove falsey values without renumbering keys or losing value tags.
#[test]
fn test_array_filter_boxed_null_callback_preserves_keys_and_types() {
    assert_filter_output(r#"<?php
function nonemptyFilter(array $values): array { return array_filter($values); }
function nullableFilter(array $values, mixed $callback): array { return array_filter($values, $callback); }
for ($i = 0; $i < 3; $i++) {
    $source = [false, "0", "", null, 0, 1.25, "word", [7]];
    $kept = nonemptyFilter($source);
    foreach ($kept as $key => $value) { echo $key, ":", gettype($value), "|"; }
    unset($key, $value);
    echo count($source), "|";
    $kept = nullableFilter(["empty" => [], "keep" => str_repeat("x", 24)], null);
    foreach ($kept as $key => $value) { echo $key, ":", strlen($value), "|"; }
    unset($source, $kept, $key, $value);
    echo count(array_filter([])), "|";
}
"#, &"5:double|6:string|7:array|8|keep:24|0|".repeat(3));
}

/// Both/key modes use logical keys and retain callback-accepted false and null values.
#[test]
fn test_array_filter_boxed_modes_and_dynamic_callback_results() {
    assert_filter_output(r#"<?php
function filterBoth(mixed $value, string $key): string { echo $key, ","; return "yes"; }
function chooseFilter(bool $which): string { return $which ? "filterBoth" : "filterBoth"; }
function modeFilter(array $values, int $mode): array {
    return array_filter($values, static function(mixed $value, mixed $key = null): bool {
        echo gettype($value), ":", gettype($key), ";"; return true;
    }, $mode);
}
for ($i = 0; $i < 3; $i++) {
    $kept = array_filter(["a" => false, "b" => null], chooseFilter($i === 0), ARRAY_FILTER_USE_BOTH);
    echo gettype($kept["a"]), ":", gettype($kept["b"]), "|";
    $kept = \ArRaY_FiLtEr(mode: ARRAY_FILTER_USE_KEY, callback: static fn(int $key): bool => $key === 8,
        array: [2 => 20, 8 => 80]);
    foreach ($kept as $key => $value) { echo $key, ":", $value, "|"; }
    unset($key, $value);
    $kept = modeFilter(["item" => 9], $i);
    echo count($kept), "|";
    unset($kept);
}
"#, "a,b,boolean:NULL|8:80|integer:NULL;1|a,b,boolean:NULL|8:80|integer:string;1|a,b,boolean:NULL|8:80|string:NULL;1|");
}

/// CUF and first-class builtin calls share defaults and nested callback descriptor ownership.
#[test]
fn test_array_filter_boxed_callable_surfaces_and_defaults() {
    assert_filter_output(r#"<?php
function filterFalse(false $value): bool { return true; }
for ($i = 0; $i < 3; $i++) {
    $filter = array_filter(...);
    $kept = $filter([0, "ok"]);
    echo $kept[1], "|";
    $kept = call_user_func("array_filter", [false, false], filterFalse(...));
    echo gettype($kept[0]), ":", count($kept), "|";
    $kept = array_filter([1, 2], static fn(int $value): bool =>
        array_any([false], static fn(false $inner): bool => !$inner));
    echo count($kept), "|";
    unset($filter, $kept);
}
"#, &"ok|boolean:2|2|".repeat(3));
}

/// Filter results keep objects alive independently and retain an iteration snapshot during source writes.
#[test]
fn test_array_filter_boxed_snapshot_and_escaping_object_ownership() {
    assert_filter_output(r#"<?php
class FilterKeptObject {
    public string $text = "owned";
    public function __destruct() { echo "released|"; }
}
for ($i = 0; $i < 3; $i++) {
    $values = ["left" => new FilterKeptObject()];
    $kept = array_filter($values);
    unset($values);
    echo $kept["left"]->text, "|";
    unset($kept);
    $values = ["a" => 1, "b" => 2];
    $kept = array_filter($values, function(int $value) use (&$values): bool {
        $values = ["replacement" => 99]; return true;
    });
    echo $kept["a"], $kept["b"], ":", $values["replacement"], "|";
    unset($values, $kept);
}
"#, &"owned|released|12:99|".repeat(3));
}

/// Throws after a kept entry retire the partial hash, callback result and temporary source owners.
#[test]
fn test_array_filter_boxed_exceptions_clean_partial_results() {
    assert_filter_output(r#"<?php
function throwingFilter(string $value, string $key): bool {
    if ($key === "bad") { throw new RuntimeException("filter failure"); }
    return true;
}
function invalidFilter(array $values, mixed $callback): void { array_filter($values, $callback); }
for ($i = 0; $i < 3; $i++) {
    try {
        array_filter(["ok" => str_repeat("x", 24), "bad" => str_repeat("y", 24)],
            throwingFilter(...), ARRAY_FILTER_USE_BOTH);
    } catch (RuntimeException $error) { echo $error->getMessage(), "|"; unset($error); }
    try { invalidFilter([], 17); }
    catch (TypeError $error) { echo "callback|"; unset($error); }
    echo count(array_filter([1])), "|";
}
"#, &"filter failure|callback|1|".repeat(3));
}

/// Legacy profiles use value-only dispatch for other integers, including negative modes.
#[test]
fn test_array_filter_php85_unknown_modes_use_values_aot_and_eval() {
    let source = r#"<?php
function legacyFilter(int $value): bool { echo $value, ","; return true; }
foreach ([-1, 3, 9] as $mode) {
    $kept = array_filter([7 => 42], legacyFilter(...), $mode);
    echo $kept[7], "|";
    echo count(array_filter([0, "ok"], null, $mode)), "|";
}
$code = 'foreach ([-1, 3, 9] as $mode) {
    $kept = array_filter([7 => 42], function($value) { echo $value, ","; return true; }, $mode);
    echo $kept[7], "|";
    echo count(array_filter([0, "ok"], null, $mode)), "|";
}';
$code = str_repeat($code, 1);
eval($code);
"#;
    assert_eq!(compile_and_run_with_php_version(source, PhpVersion::Php85), "42,42|1|".repeat(6));
}

/// PHP 8.6 validates modes even for empty arrays and null callbacks, on AOT and eval paths.
#[test]
fn test_array_filter_php86_unknown_modes_throw_aot_and_eval() {
    let source = r#"<?php
foreach ([-1, 3, 9] as $mode) {
    try { array_filter([], null, $mode); echo "bad"; }
    catch (ValueError $error) { echo "mode|"; }
}
$code = 'foreach ([-1, 3, 9] as $mode) {
    try { array_filter([], null, $mode); echo "bad"; }
    catch (ValueError $error) { echo "mode|"; }
}';
$code = str_repeat($code, 1);
eval($code);
"#;
    assert_eq!(compile_and_run_with_php_version(source, PhpVersion::Php86), "mode|".repeat(6));
}
