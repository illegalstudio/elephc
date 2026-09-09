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
    assert_eq!(compile_and_run(source), "cast|dropped|10,30");
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
