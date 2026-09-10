//! Purpose:
//! Verifies boxed array_udiff and array_uintersect behavior and ownership.
//!
//! Called from:
//! - The runtime GC codegen suite on every executable target.
//!
//! Key details:
//! - Fixtures cover keyed heterogeneous arrays, callable surfaces, snapshots and exceptions.
//! - Heap-debug and tagged executions must agree without leaking temporary operand owners.

use crate::support::*;

/// Declared heterogeneous arrays preserve sparse keys, values and independent result ownership.
#[test]
fn test_core_boxed_set_comparators_preserve_keys_and_values() {
    assert_clean_set_comparator(r#"<?php
class SetComparatorValue {
    public int $id;
    public function __construct(int $id) { $this->id = $id; }
}
function setComparatorToken(mixed $value): string {
    if (is_object($value)) { return "object"; }
    if (is_array($value)) { return "array:" . count($value); }
    return gettype($value) . ":" . strval($value);
}
function compareSetValues(mixed $left, mixed $right): int {
    return strcmp(setComparatorToken($left), setComparatorToken($right));
}
function declaredSetValues(): array {
    return [
        0 => "same",
        4 => 2.5,
        "object" => new SetComparatorValue(4),
        "nested" => ["id" => 7],
        "keep" => "unique",
    ];
}
$first = declaredSetValues();
$second = ["same", 2.5, new SetComparatorValue(4), ["id" => 7]];
$difference = array_udiff($first, $second, "compareSetValues");
$intersection = array_uintersect($first, $second, compareSetValues(...));
echo implode(",", array_keys($difference)), "|";
echo implode(",", array_keys($intersection)), "|";
$intersection["nested"]["id"] = 8;
echo $first["nested"]["id"], ":", $intersection["nested"]["id"], "|";
unset($first, $second);
echo $intersection["object"]->id, ":", $intersection["nested"]["id"];
unset($difference, $intersection);
"#, "keep|0,4,object,nested|7:8|4:8");
}

/// Direct, named, CUF, FCC and runtime-selected callable routes share one boxed result contract.
#[test]
fn test_core_boxed_set_comparator_callable_surfaces() {
    assert_clean_set_comparator(r#"<?php
function compareSetIntegers(int $left, int $right): int { return $left <=> $right; }
function invokeSetOperation(callable $operation, array $left, array $right, mixed $compare): array {
    return $operation($left, $right, $compare);
}
$difference = array_udiff(...);
$intersection = array_uintersect(...);
$runtime = $argc === 1 ? $difference : $intersection;
echo count(array_udiff([1, 2, 3], [2], "compareSetIntegers")), "|";
echo count(array_uintersect(array1: [1, 2, 3], array2: [2], callback: compareSetIntegers(...))), "|";
echo count(call_user_func("array_udiff", [1, 2, 3], [2], "compareSetIntegers")), "|";
echo count($intersection([1, 2, 3], [2], "compareSetIntegers")), "|";
echo count(invokeSetOperation($runtime, [1, 2, 3], [2], compareSetIntegers(...))), "|";
echo count(\ARRAY_UDIFF(...["callback" => "compareSetIntegers", "array2" => [2], "array1" => [1, 2, 3]]));
unset($difference, $intersection, $runtime);
"#, "2|1|2|1|2|2");
}

/// Comparator results are cast to int before zero is interpreted as equality.
#[test]
fn test_core_boxed_set_comparator_integer_coercion() {
    assert_clean_set_comparator(r#"<?php
echo count(array_udiff([], [1], static fn(int $left, int $right): int => $left <=> $right)), "|";
echo count(array_uintersect([1], [], static fn(int $left, int $right): int => $left <=> $right)), "|";
echo count(array_udiff([1], [2], static fn(int $left, int $right): float => 0.75)), "|";
echo count(array_uintersect([1], [2], static fn(int $left, int $right): string => "0")), "|";
echo count(array_udiff([1], [2], static fn(int $left, int $right): bool => false)), "|";
echo count(array_uintersect([1], [2], static fn(int $left, int $right): string => "-1.5"));
"#, "0|0|0|1|0|0");
}

/// Empty arrays still validate dynamic source and callback operands with catchable TypeErrors.
#[test]
fn test_core_boxed_set_comparator_invalid_operands() {
    assert_clean_set_comparator(r#"<?php
function rejectSetSource(mixed $source): void {
    try { array_udiff($source, [], static fn($a, $b): int => 0); echo "missed|"; }
    catch (TypeError $error) { echo "first|"; unset($error); }
    try { array_uintersect([], $source, static fn($a, $b): int => 0); echo "missed|"; }
    catch (TypeError $error) { echo "second|"; unset($error); }
}
function rejectSetCallback(mixed $callback): void {
    try { array_udiff([], [], $callback); echo "missed|"; }
    catch (TypeError $error) { echo "callback|"; unset($error); }
}
rejectSetSource(null);
rejectSetSource(7);
rejectSetCallback(null);
rejectSetCallback("missingSetComparator");
rejectSetCallback([]);
echo "done";
"#, "first|second|first|second|callback|callback|callback|done");
}

/// Source snapshots survive callback mutation and callback throws retire every published owner.
#[test]
fn test_core_boxed_set_comparator_snapshot_and_throw_cleanup() {
    assert_clean_set_comparator(r#"<?php
class SetComparatorOwner {
    public string $value;
    public function __construct(string $value) { $this->value = $value; }
}
$left = ["a" => "one", "b" => "two"];
$right = ["two"];
$compare = function(string $first, string $second) use (&$left, &$right): int {
    $left = ["replacement"];
    $right = [];
    return strcmp($first, $second);
};
$kept = array_uintersect($left, $right, $compare);
echo implode(",", array_keys($kept)), ":", $kept["b"], "|";
try {
    array_udiff(
        [new SetComparatorOwner("left")],
        [new SetComparatorOwner("right")],
        static function(SetComparatorOwner $first, SetComparatorOwner $second): int {
            throw new RuntimeException($first->value . ":" . $second->value);
        },
    );
    echo "missed|";
} catch (RuntimeException $error) {
    echo $error->getMessage(), "|";
    unset($error);
}
echo $left[0], ":", count($right);
unset($kept, $compare, $left, $right);
"#, "b:two|left:right|replacement:0");
}

/// Concrete and declared temporary sources retire independently from the escaping result.
#[test]
fn test_core_boxed_set_comparator_temporary_source_ownership() {
    assert_clean_set_comparator(r#"<?php
function temporarySetWords(): array { return ["first" => "a", "last" => "b"]; }
function compareSetWords(string $left, string $right): int { return strcmp($left, $right); }
$raw = array_udiff(["a", "b"], ["b"], "compareSetWords");
$boxed = array_uintersect(temporarySetWords(), ["b"], "compareSetWords");
echo $raw[0], ":", implode(",", array_keys($boxed)), ":", $boxed["last"];
unset($raw, $boxed);
"#, "a:last:b");
}

/// Checks observable output and clean heap ownership in both native runtime modes.
fn assert_clean_set_comparator(source: &str, expected: &str) {
    let (out, assembly) = compile_and_run_with_heap_debug_and_asm(source);
    assert!(out.success, "stdout={:?}\nstderr={}\n{assembly}", out.stdout, out.stderr);
    assert_eq!(out.stdout, expected, "{}\n{assembly}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}\n{assembly}", out.stderr);
    assert_eq!(compile_and_run_tagged(source), expected);
}
