//! Purpose:
//! Covers prepending to boxed PHP arrays across reference, COW and payload ownership boundaries.
//!
//! Called from:
//! - The runtime GC codegen test module.
//!
//! Key details:
//! - Numeric keys are reindexed while string keys and nested value owners survive rebuilding.

use crate::support::*;

/// Declared reference parameters publish growth and key renumbering without changing COW aliases.
#[test]
fn test_boxed_array_unshift_reference_growth_and_key_renumbering() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function prependMany(array &$values): int { return array_unshift($values, 9, 8, 7, 6, 5, 4, 3, 2, 1); }
function prependText(array &$values): int { return array_unshift($values, str_repeat("p", 3)); }
function reindexOnly(array &$values): int { return array_unshift($values); }
$numbers = [1, 2];
$alias = $numbers;
echo prependMany($numbers), ":", implode(",", $numbers), "|", implode(",", $alias), "|";
$map = [7 => "seven", "name" => "kept", -2 => "negative"];
$mapAlias = $map;
echo prependText($map), ":", implode(",", array_keys($map)), ":", implode(",", array_values($map)), "|";
echo implode(",", array_keys($mapAlias)), "|";
$keys = [8 => 80, "label" => 10, 3 => 30];
echo reindexOnly($keys), ":", implode(",", array_keys($keys)), "|";
$empty = [];
echo reindexOnly($empty), ":", prependText($empty), ":", $empty[0];
unset($numbers, $alias, $map, $mapAlias, $keys, $empty);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "11:9,8,7,6,5,4,3,2,1,1,2|1,2|4:0,1,name,2:ppp,seven,kept,negative|7,name,-2|3:0,label,1|0:1:ppp", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Self-prepending captures the old array rather than creating a cycle in the replacement cell.
#[test]
fn test_boxed_array_unshift_self_snapshot_and_heap_payloads() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class PrependedOwner {
    public string $name;
    public function __construct(string $name) { $this->name = $name; }
    public function __destruct() { echo "drop:", $this->name, "|"; }
}
function prependSelf(array &$values): int { return array_unshift($values, $values); }
function prependOwners(array &$values, PrependedOwner $object): int {
    return array_unshift($values, $object, [str_repeat("n", 3)], false, 1.25);
}
$values = [1, 2];
echo prependSelf($values), ":", count($values[0]), ":", $values[0][1], "|";
$object = new PrependedOwner(str_repeat("o", 3));
$owners = [str_repeat("t", 3)];
echo prependOwners($owners, $object), "|";
unset($object);
echo $owners[0]->name, ":", $owners[1][0], ":", gettype($owners[2]), ":", $owners[3], ":", $owners[4], "|";
unset($values, $owners);
echo "done";
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "3:2:2|5|ooo:nnn:boolean:1.25:ttt|drop:ooo|done", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A prepended object is owned only by the rebuilt array after caller and callee temporaries retire.
#[test]
fn test_boxed_array_unshift_object_owner_retires_with_array() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class PrependedOnlyOwner {
    public function __destruct() { echo "drop|"; }
}
function prependOnlyOwner(array &$values, PrependedOnlyOwner $object): void {
    array_unshift($values, $object);
}
$object = new PrependedOnlyOwner();
$values = [];
prependOnlyOwner($values, $object);
unset($object, $values);
echo "done";
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "drop|done", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Reading an object property from a boxed array does not retain the object past array teardown.
#[test]
fn test_boxed_array_object_property_read_does_not_extend_owner() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class BoxedPropertyOwner {
    public string $name = "kept";
    public function __destruct() { echo "drop|"; }
}
$object = new BoxedPropertyOwner();
$values = ["seed" => str_repeat("t", 3), 0 => $object, 1 => false, 2 => 1.25];
unset($object);
echo $values[0]->name, "|";
unset($values);
echo "done";
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "kept|drop|done", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A concrete Mixed-valued hash prepends values, renumbers integer keys, keeps a copied alias
/// intact, and releases every owner.
#[test]
fn test_core_concrete_mixed_hash_unshift_renumbers_and_balances() {
    let out = compile_and_run_with_heap_debug(r#"<?php
for ($i = 0; $i < 4; $i++) {
    $items = [1 => str_repeat('a', 12), 2 => 'b', 5 => 3];
    $copy = $items;
    $count = array_unshift($items, str_repeat('z', 12), 9);
    echo $count, '|', strlen($items[0]), '|', $items[1], '|', implode(',', $items), '|', count($copy), '|';
    unset($items, $copy);
}
echo 'done';
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "5|12|9|zzzzzzzzzzzz,9,aaaaaaaaaaaa,b,3|3|".repeat(4) + "done", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A nested mutating argument republishes the receiver's table; `array_unshift` must reload it and
/// stay heap-balanced.
#[test]
fn test_core_concrete_mixed_hash_unshift_reloads_receiver_after_argument_mutation() {
    let out = compile_and_run_with_heap_debug(r#"<?php
for ($i = 0; $i < 4; $i++) {
    $items = ["k1" => str_repeat('a', 12), "k2" => "y", "z" => 3];
    array_unshift($items, array_shift($items));
    echo count($items), '|', strlen($items[0]), '|', implode(',', $items), '|';
    unset($items);
}
echo 'done';
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(
        out.stdout,
        "3|12|aaaaaaaaaaaa,y,3|".repeat(4) + "done",
        "{}",
        out.stderr
    );
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A later argument replacing a concrete hash with null raises TypeError before prefix construction.
#[test]
fn test_core_concrete_mixed_hash_unshift_checks_reloaded_null_receiver() {
    let out = compile_and_run_with_heap_debug(r#"<?php
$conf = ['ok' => ['x' => 1, 'y' => 's']];
$a = ['x' => 1, 'y' => 's'];
try { array_unshift($a, $a = $conf['missing']); }
catch (TypeError $error) { echo "T:", $error->getMessage(), "\n"; unset($error); }
echo "after:", (is_null($a) ? "NULL" : "BAD"), "\n";
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "T:array_unshift(): Argument #1 ($array) must be of type array, null given\nafter:NULL\n", "{}", out.stderr);
    assert_eq!(out.stderr.matches("Warning: Undefined array key \"missing\"").count(), 1, "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A formerly null hash becomes valid while later arguments run and must pass the live guard.
#[test]
fn test_core_concrete_mixed_hash_unshift_accepts_reloaded_array_receiver() {
    let out = compile_and_run_with_heap_debug(r#"<?php
$conf = ['ok' => ['x' => 1, 'y' => 's']];
$a = $conf['missing'];
try { $count = array_unshift($a, $a = $conf['ok']); echo $count, '|', json_encode($a), '|', gettype($a), "\n"; }
catch (TypeError $error) { echo 'BAD:', $error->getMessage(); unset($error); }
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "3|{\"0\":{\"x\":1,\"y\":\"s\"},\"x\":1,\"y\":\"s\"}|array\n", "{}", out.stderr);
    assert_eq!(out.stderr.matches("Warning: Undefined array key \"missing\"").count(), 1, "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Later arguments run before the live guard and their owned values unwind without damaging a copy.
#[test]
fn test_core_concrete_mixed_hash_unshift_rejects_null_after_later_arguments() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function later(): mixed { echo 'L;'; return str_repeat('payload', 3); }
$conf = ['ok' => ['x' => 1, 'y' => 's']];
$a = ['x' => 1, 'y' => 's']; $copy = $a;
try { array_unshift($a, $a = $conf['missing'], later()); }
catch (TypeError $error) { echo 'C;'; unset($error); }
echo (is_null($a) ? "NULL" : "BAD"), '|', json_encode($copy), "\n";
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "L;C;NULL|{\"x\":1,\"y\":\"s\"}\n", "{}", out.stderr);
    assert_eq!(out.stderr.matches("Warning: Undefined array key \"missing\"").count(), 1, "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A reference alias must guard its current pointee and leave both aliases null after refusal.
#[test]
fn test_core_concrete_mixed_hash_unshift_ref_cell_checks_reloaded_null() {
    let out = compile_and_run_with_heap_debug(r#"<?php
$conf = ['ok' => ['x' => 1, 'y' => 's']];
$a = ['x' => 1, 'y' => 's']; $alias =& $a;
try { array_unshift($alias, $a = $conf['missing']); }
catch (TypeError $error) { echo get_class($error), ';'; unset($error); }
echo (is_null($a) ? "NULL" : "BAD"), ':', (is_null($alias) ? "NULL" : "BAD"), "\n";
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "TypeError;NULL:NULL\n", "{}", out.stderr);
    assert_eq!(out.stderr.matches("Warning: Undefined array key \"missing\"").count(), 1, "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// An array parameter passed by reference uses the same live guard before prefix ownership exists.
#[test]
fn test_core_concrete_mixed_hash_unshift_array_parameter_checks_reloaded_null() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function change(array &$a): void {
    $conf = ['ok' => ['x' => 1, 'y' => 's']];
    try { array_unshift($a, $a = $conf['missing']); }
    catch (TypeError $error) { echo get_class($error), ';'; unset($error); }
    echo gettype($a), "\n";
}
$a = ['x' => 1, 'y' => 's']; change($a);
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "TypeError;NULL\n", "{}", out.stderr);
    assert_eq!(out.stderr.matches("Warning: Undefined array key \"missing\"").count(), 1, "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// No-value unshift renumbers numeric keys without changing either hash value representation.
#[test]
fn test_core_concrete_mixed_hash_unshift_zero_values_reindex_mixed_and_typed_hashes() {
    let out = compile_and_run_with_heap_debug(r#"<?php
$mixed = [8 => str_repeat('x', 12), 'label' => 17, 3 => 'tail']; $mixedCopy = $mixed;
echo array_unshift($mixed), '|', json_encode(array_keys($mixed)), '|', json_encode(array_keys($mixedCopy)), '|', implode(',', $mixed), "\n";
$ints = [8 => 80, 'label' => 10, 3 => 30]; $intCopy = $ints;
array_unshift($ints);
echo count($ints), '|', json_encode(array_keys($ints)), '|', json_encode(array_keys($intCopy)), '|', implode(',', $ints), "\n";
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "3|[0,\"label\",1]|[8,\"label\",3]|xxxxxxxxxxxx,17,tail\n3|[0,\"label\",1]|[8,\"label\",3]|80,10,30\n", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A no-value call through an alias resets append history while preserving a surviving copy.
#[test]
fn test_core_concrete_mixed_hash_unshift_zero_values_ref_cell_resets_append_history() {
    let out = compile_and_run_with_heap_debug(r#"<?php
$items = [17 => str_repeat('gone', 3), 'label' => 'stay']; unset($items[17]);
$alias =& $items; $copy = $items;
array_unshift($alias); $items[] = 'tail'; $copy[] = 'old';
echo count($items), '|', json_encode($items), '|', json_encode($copy), "\n";
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "2|{\"label\":\"stay\",\"0\":\"tail\"}|{\"label\":\"stay\",\"18\":\"old\"}\n", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// An emptied concrete hash remains an array and restarts numeric appends at zero.
#[test]
fn test_core_concrete_mixed_hash_unshift_zero_values_empty_hash_stays_valid() {
    let out = compile_and_run_with_heap_debug(r#"<?php
$items = ['x' => 1, 'y' => 's']; unset($items['x']); unset($items['y']);
echo array_unshift($items), '|'; $items[] = 9;
echo implode(',', array_keys($items)), '|', implode(',', $items), "\n";
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "0|0|9\n", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A typed object hash retains every child through reindexing and releases it with the last copy.
#[test]
fn test_core_concrete_mixed_hash_unshift_zero_values_preserves_object_owners() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class Job {
    public int $id;
    public function __construct(int $id) { $this->id = $id; }
    public function __destruct() { echo 'D', $this->id, ';'; }
}
$jobs = [9 => new Job(1), 'keep' => new Job(2)]; $copy = $jobs;
array_unshift($jobs);
echo 'A:', implode(',', array_keys($jobs)), '|', $jobs[0]->id, '|', $jobs['keep']->id, '|', implode(',', array_keys($copy)), ';';
unset($jobs); echo 'B;'; unset($copy[9]); unset($copy['keep']); unset($copy); echo "C\n";
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "A:0,keep|1|2|9,keep;B;D1;D2;C\n", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
