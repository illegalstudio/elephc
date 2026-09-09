//! Purpose:
//! Verifies joining boxed and promoted PHP arrays without assuming a dense payload.
//!
//! Called from:
//! - The runtime GC codegen integration suite.
//!
//! Key details:
//! - Normalized arrays and persisted string results own independent storage.
//! - Throwing string conversions must release temporary array owners before catch resumes.

use crate::support::*;

/// Declared arrays join packed scalars and sparse hashes without changing their keys or copies.
#[test]
fn test_core_php_array_implode_normalizes_packed_and_sparse_values() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function describeJoinedArray(array $items): void {
    echo implode(",", array_keys($items)), ":", implode(",", $items), ":", join($items), "|";
}
describeJoinedArray([10, 20]);
describeJoinedArray([true, false, true]);
describeJoinedArray([1.5, 2.5]);
describeJoinedArray(["left" => "a", 8 => "b"]);
describeJoinedArray([]);
$items = [10, 20, 30];
$copy = $items;
unset($items[1]);
describeJoinedArray($items);
describeJoinedArray($copy);
unset($items, $copy);
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "0,1:10,20:1020|0,1,2:1,,1:11|0,1:1.5,2.5:1.52.5|left,8:a,b:ab|::|0,2:10,30:1030|0,1,2:10,20,30:102030|", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Mixed-slot arrays promoted by a keyed write still join their values in insertion order.
#[test]
fn test_core_implode_promoted_mixed_slots_and_result_owners_stay_clean() {
    let out = compile_and_run_with_heap_debug(r#"<?php
$values = [1, "two", true];
$values["named"] = "last";
$copy = $values;
unset($values[1]);
for ($i = 0; $i < 20; $i++) {
    $joined = implode(":", $values);
    if ($joined !== "1:1:last") { echo "bad"; }
    unset($joined);
}
echo join($values), "|", implode(":", $copy);
unset($values, $copy);
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "11last|1:two:1:last", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A throwing element conversion releases the normalized array so its object can die in catch's caller.
#[test]
fn test_core_php_array_implode_throw_releases_normalized_owner() {
    let source = r#"<?php
class ThrowingJoinedElement {
    public function __toString(): string { throw new RuntimeException("cast"); }
    public function __destruct() { echo "dropped|"; }
}
function joinThrowingArray(array $items): string { return implode(",", $items); }
$values = ["object" => new ThrowingJoinedElement()];
try { echo joinThrowingArray($values); }
catch (RuntimeException $error) { echo $error->getMessage(), "|"; unset($error); }
unset($values);
echo joinThrowingArray([0 => 10, 2 => 30]);
"#;
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "cast|dropped|10,30", "{}", out.stderr);
}

/// Direct joins isolate conversion unwinding from the extra by-value PHP array call boundary.
#[test]
fn test_core_implode_direct_throw_releases_element_object() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class DirectThrowingJoinElement {
    public function __toString(): string { throw new RuntimeException("cast"); }
    public function __destruct() { echo "dropped|"; }
}
$values = ["object" => new DirectThrowingJoinElement(), "tail" => 1];
try { echo implode(",", $values); }
catch (RuntimeException $error) { echo $error->getMessage(), "|"; unset($error); }
unset($values);
echo "done";
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "cast|dropped|done", "{}", out.stderr);
}

/// Throwing after binding an array parameter must retire its shadow without retaining child objects.
#[test]
fn test_core_php_array_throw_releases_parameter_shadow_without_join() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class ThrowingArrayBoundaryElement {
    public function __destruct() { echo "dropped|"; }
}
function throwAfterArrayBinding(array $items): void {
    echo count($items), ":";
    throw new RuntimeException("bound");
}
$values = ["object" => new ThrowingArrayBoundaryElement()];
try { throwAfterArrayBinding($values); }
catch (RuntimeException $error) { echo $error->getMessage(), "|"; unset($error); }
unset($values);
echo "done";
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "1:bound|dropped|done", "{}", out.stderr);
}

/// Returning a fresh join or placing it before a call in a concat must not leak a duplicate string.
#[test]
fn test_core_php_array_implode_return_and_concat_transfer_owned_strings() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function ownedJoin(array $items): string { return implode(",", $items); }
function joinSuffix(int $value): string { return "tail" . $value; }
function ownedJoinConcat(array $items, int $value): string {
    return implode(",", $items) . joinSuffix($value);
}
$sum = 0;
for ($i = 0; $i < 30; $i++) {
    $joined = ownedJoin([1, 2]);
    $combined = ownedJoinConcat([3, 4], $argc);
    $sum += strlen($joined) + strlen($combined);
    unset($joined, $combined);
}
echo $sum;
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "330", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Invalid dynamic inputs throw before reading a container header and do not poison later joins.
#[test]
fn test_core_implode_mixed_non_array_throws_type_error() {
    let source = r#"<?php
function joinUnknownValue(mixed $items): string { return implode(",", $items); }
function joinOneUnknownValue(mixed $items): string { return join($items); }
try { echo joinUnknownValue(42); }
catch (TypeError $error) { echo "two|"; }
try { echo joinOneUnknownValue("invalid"); }
catch (TypeError $error) { echo "one|"; }
echo joinUnknownValue(["a" => "kept"]);
"#;
    assert_eq!(compile_and_run(source), "two|one|kept");
}

/// Fresh results survive input destructors that reenter string rendering during owner retirement.
#[test]
fn test_core_php_array_implode_result_survives_rendering_destructor() {
    let source = r#"<?php
class JoinedElementStore { public static array $items = []; }
class RemovingJoinedElement {
    public function __toString(): string { JoinedElementStore::$items = []; return "kept"; }
    public function __destruct() { echo implode(",", [1, 2]), "|"; }
}
JoinedElementStore::$items = ["object" => new RemovingJoinedElement()];
echo implode(",", JoinedElementStore::$items);
"#;
    assert_eq!(compile_and_run(source), "1,2|kept");
}

/// Object hooks may reenter join rendering without overwriting the outer prefix or glue.
#[test]
fn test_core_php_array_implode_object_hook_preserves_prefix_and_owned_results() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class NestedJoinedValue {
    public function __toString(): string {
        $nested = implode("-", ["inner", "value"]);
        return $nested;
    }
}
class InheritedJoinedValue extends NestedJoinedValue {}
function joinObjects(array $items): string { return implode(":", $items); }
for ($i = 0; $i < 10; $i++) {
    $result = joinObjects(["before", new InheritedJoinedValue(), "after"]);
    if ($result !== "before:inner-value:after") { echo "bad"; }
    unset($result);
}
echo "done";
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "done", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A non-stringable object must throw a catchable Error instead of silently contributing empty bytes.
#[test]
fn test_core_php_array_implode_non_stringable_object_throws_error() {
    let source = r#"<?php
class NonStringableJoinedValue {}
function joinInvalidObject(array $items): string { return implode(",", $items); }
try { echo joinInvalidObject([new NonStringableJoinedValue()]); }
catch (Error $error) { echo "invalid|"; }
echo joinInvalidObject(["kept"]);
"#;
    assert_eq!(compile_and_run(source), "invalid|kept");
}

/// Array-to-string warning handlers can render nested joins without corrupting the outer prefix.
#[test]
fn test_core_php_array_implode_warning_handler_preserves_prefix() {
    let source = r#"<?php
set_error_handler(function(int $level, string $message): bool {
    echo implode(",", ["warn", "handled"]), "|";
    return true;
});
function joinWarningArray(array $items): string { return implode(":", $items); }
echo joinWarningArray([1, ["nested"], 2]);
restore_error_handler();
"#;
    assert_eq!(compile_and_run(source), "warn,handled|1:Array:2");
}
