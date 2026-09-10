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
