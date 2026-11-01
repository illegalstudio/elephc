//! Purpose:
//! Integration tests for associative-array set/merge builtins such as `array_replace` and
//! `array_unique`.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries; assertions compare stdout.
//! - Covers in-place overwrite (keeping key position), appended keys, copy-on-write
//!   non-mutation of the source, string values, and case-insensitive calls.

use crate::support::*;

/// Verifies the two-hash set builtins promote a gradual operand only when it is not ALREADY a
/// hash at runtime.
///
/// `array<mixed>` is what a declared `array` parameter carries, and it says nothing about keys.
/// Reading it as "indexed, needs promoting" walked hash storage as if it were indexed, so
/// `array_diff_key(['x'=>1,'y'=>2], ['x'=>1])` answered `2:0,1:7,1024` — invented keys AND
/// invented values — where php answers `1:y:2`.
#[test]
fn test_two_hash_set_builtins_promote_a_gradual_operand_only_when_indexed() {
    let output = compile_and_run(
        r#"<?php
function shape(array $a): string {
    return count($a) . ":" . implode(",", array_keys($a)) . ":" . implode(",", array_values($a));
}
function diffKey(array $a): string {
    return shape(array_diff_key($a, ["x" => 1]));
}
function intersectKey(array $a): string {
    return shape(array_intersect_key($a, ["x" => 1]));
}
echo diffKey(["x" => 1, "y" => 2]), "|", diffKey([7, 8]), "|";
echo intersectKey(["x" => 1, "y" => 2]), "|", intersectKey([7, 8]);
"#,
    );
    assert_eq!(output, "1:y:2|2:0,1:7,8|1:x:1|0::");
}

/// Verifies null values can be stored in and strictly searched within a mixed associative array.
#[test]
fn test_assoc_mixed_null_storage_and_membership() {
    let output = compile_and_run(
        r#"<?php
$values = ["present" => null, "other" => 1];
echo in_array(null, $values, true) ? "yes" : "no";
"#,
    );
    assert_eq!(output, "yes");
}

/// Verifies spread-only `array_merge()` retains its compiler-provided helper
/// through declaration pruning and preserves PHP's integer-key renumbering.
#[test]
fn test_array_merge_spread_helper_survives_declaration_pruning() {
    let out = compile_and_run(
        r#"<?php
$arrays = [[10, 11], [12], [13, 14]];
foreach (array_merge(...$arrays) as $value) {
    echo $value;
}
"#,
    );

    assert_eq!(out, "1011121314");
}

/// Verifies spread-only `array_merge()` flattens nested arrays held in a
/// gradual associative outer value without dropping any integer-keyed entries.
#[test]
fn test_array_merge_spread_flattens_gradual_associative_nested_arrays() {
    let out = compile_and_run(
        r#"<?php
function preserve_as_mixed(mixed $value): mixed {
    return $value;
}

$groups = preserve_as_mixed([0 => [10, 11], -2048 => [12]]);
foreach (array_merge(...$groups) as $value) {
    echo $value;
}
"#,
    );

    assert_eq!(out, "101112");
}

/// Verifies a typed property copied, key-sorted, and spread into `array_merge()`
/// keeps its nested arrays visible through the gradual helper boundary.
#[test]
fn test_array_merge_spread_after_key_sort_of_typed_nested_array_property() {
    let out = compile_and_run(
        r#"<?php
class PriorityGroups {
    public array $groups;

    public function __construct() {
        $this->groups = [0 => [10, 11], -2048 => [12]];
    }

    public function flatten(): array {
        $groups = $this->groups;
        krsort($groups);
        return array_merge(...$groups);
    }
}

foreach ((new PriorityGroups())->flatten() as $value) {
    echo $value;
}
"#,
    );

    assert_eq!(out, "101112");
}

/// Verifies a gradual `array_merge()` retains the runtime class identity of object elements.
///
/// The helper returns `array<mixed>`, so the subsequent foreach and method call exercise the
/// boxed-object bridge instead of relying on a concrete array element representation.
#[test]
fn test_gradual_array_merge_preserves_object_method_dispatch() {
    let out = compile_and_run(
        r#"<?php
class MergeDispatchProbe {
    public function process(): string { return "processed"; }
}
function gradual(mixed $value): mixed { return $value; }
$first = gradual([new MergeDispatchProbe()]);
$result = array_merge($first, gradual([]));
foreach ($result as $pass) {
    echo get_class($pass), ":", $pass->process();
}
"#,
    );

    assert_eq!(out, "MergeDispatchProbe:processed");
}

/// Verifies `array_unique()` preserves the first associative keys for gradual Mixed values.
#[test]
fn test_array_unique_assoc_mixed_preserves_first_keys() {
    let out = compile_and_run(
        r#"<?php
function gradual_unique_value(mixed $value): mixed { return $value; }
$values = [
    "first" => gradual_unique_value("alpha"),
    "second" => gradual_unique_value("beta"),
    "duplicate" => gradual_unique_value("alpha"),
];
$result = array_unique($values);
echo count($result), ":";
foreach ($result as $key => $value) {
    echo $key, "=", $value, ";";
}
"#,
    );
    assert_eq!(out, "2:first=alpha;second=beta;");
}

/// Verifies array_replace() overwrites matching keys in place and appends new keys,
/// preserving the first array's key order.
/// Fixture: base {a:1,b:2} replaced by {b:9,c:3} → a=1;b=9;c=3 in that order.
#[test]
fn test_array_replace_overwrite_and_append() {
    let out = compile_and_run(
        r#"<?php
$base = ["a" => 1, "b" => 2];
$over = ["b" => 9, "c" => 3];
$r = array_replace($base, $over);
foreach ($r as $k => $v) { echo "$k=$v;"; }
"#,
    );
    assert_eq!(out, "a=1;b=9;c=3;");
}

/// Verifies array_replace() does not mutate the source array (copy-on-write).
/// Fixture: base {x:1} replaced by {x:5}; base["x"] stays 1, result["x"] is 5.
#[test]
fn test_array_replace_source_unchanged() {
    let out = compile_and_run(
        r#"<?php
$base = ["x" => 1];
$over = ["x" => 5];
$r = array_replace($base, $over);
echo $base["x"];
echo $r["x"];
"#,
    );
    assert_eq!(out, "15");
}

/// Verifies array_replace() carries string values from both arrays into the result.
/// Fixture: base {name:"alice",role:"user"} replaced by {role:"admin"}.
#[test]
fn test_array_replace_string_values() {
    let out = compile_and_run(
        r#"<?php
$base = ["name" => "alice", "role" => "user"];
$over = ["role" => "admin"];
$r = array_replace($base, $over);
echo $r["name"];
echo "-";
echo $r["role"];
"#,
    );
    assert_eq!(out, "alice-admin");
}

/// Verifies `array_replace()` normalizes boxed gradual array operands to one Mixed-valued hash
/// layout while preserving the source arrays and balanced temporary ownership.
#[test]
fn test_array_replace_gradual_mixed_operands() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function gradual_array(mixed $value): mixed { return $value; }
$base = gradual_array(["name" => "alice", "role" => "user"]);
$over = gradual_array(["role" => "admin", "count" => 2]);
$result = array_replace($base, $over);
echo $result["name"], "|", $result["role"], "|", $result["count"], "|", $base["role"];
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "alice|admin|2|user");
}

/// Verifies `array_replace()` promotes an indexed Mixed-valued operand when another operand uses
/// gradual associative storage.
#[test]
fn test_array_replace_mixed_hash_and_indexed_operand() {
    let out = compile_and_run(
        r#"<?php
function gradual_replace_value(mixed $value): mixed { return $value; }
function gradual_replace_array(mixed $value): mixed { return $value; }
$base = gradual_replace_array(["name" => "alice"]);
$indexed = [gradual_replace_value("first"), gradual_replace_value("second")];
$result = array_replace($base, $indexed);
echo $result["name"], "|", $result[0], "|", $result[1];
"#,
    );
    assert_eq!(out, "alice|first|second");
}

/// Verifies `array_replace()` promotes two indexed Mixed-valued operands while retaining heap
/// values and preserving numeric-key replacement semantics.
#[test]
fn test_array_replace_indexed_mixed_operands() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function gradual_replace_item(mixed $value): mixed { return $value; }
$base = [gradual_replace_item("first"), gradual_replace_item(["keep" => 2])];
$over = [gradual_replace_item("changed")];
$result = array_replace($base, $over);
echo $result[0], "|", $result[1]["keep"], "|", $base[0];
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "changed|2|first");
}

/// Verifies `array_replace()` accepts indexed string arrays and preserves untouched heap values.
#[test]
fn test_array_replace_indexed_string_operands() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$result = array_replace(["first", "keep"], ["changed"]);
echo $result[0], "|", $result[1];
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "changed|keep");
}

/// Verifies key-set operations normalize gradual and indexed operands to associative storage.
#[test]
fn test_array_key_set_ops_with_gradual_and_indexed_operands() {
    let out = compile_and_run(
        r#"<?php
function gradual_key_array(mixed $value): mixed { return $value; }
$left = gradual_key_array([0 => "zero", 1 => "one", "name" => "alice"]);
$mask = [gradual_key_array(true)];
$diff = array_diff_key($left, $mask);
$intersect = array_intersect_key($left, $mask);
echo count($diff), "|", $diff[1], "|", $diff["name"], "|", count($intersect), "|", $intersect[0];
"#,
    );
    assert_eq!(out, "2|one|alice|1|zero");
}

/// Verifies key-set operations convert indexed Mixed operands to hashes so removed integer keys
/// retain their original positions instead of being renumbered.
#[test]
fn test_array_key_set_ops_with_indexed_mixed_operands() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function gradual_key_item(mixed $value): mixed { return $value; }
$left = [gradual_key_item("zero"), gradual_key_item("one")];
$mask = [gradual_key_item(true)];
$diff = array_diff_key($left, $mask);
$intersect = array_intersect_key($left, $mask);
foreach ($diff as $key => $value) { echo $key, ":", $value; }
echo "|";
foreach ($intersect as $key => $value) { echo $key, ":", $value; }
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "1:one|0:zero");
}

/// Verifies array_replace() result count reflects merged distinct keys.
/// Fixture: {a:1,b:2} replaced by {b:9,c:3,d:4} → 4 distinct keys.
#[test]
fn test_array_replace_count() {
    let out = compile_and_run(
        r#"<?php
$r = array_replace(["a" => 1, "b" => 2], ["b" => 9, "c" => 3, "d" => 4]);
echo count($r);
"#,
    );
    assert_eq!(out, "4");
}

/// Verifies array_replace() is callable case-insensitively, matching PHP builtin name rules.
/// Fixture: mixed-case spelling overwriting one key.
#[test]
fn test_array_replace_case_insensitive() {
    let out = compile_and_run(
        r#"<?php
$r = Array_Replace(["a" => 1], ["a" => 2]);
echo $r["a"];
"#,
    );
    assert_eq!(out, "2");
}

/// Verifies array_diff_assoc() keeps entries whose (key, value) pair is absent from the
/// second array: matching pair dropped, differing value kept, missing key kept.
/// Fixture: {a:1,b:2,c:3} vs {a:1,b:5} → b=2 (value differs) and c=3 (key absent) remain.
#[test]
fn test_array_diff_assoc_basic() {
    let out = compile_and_run(
        r#"<?php
$a = ["a" => 1, "b" => 2, "c" => 3];
$b = ["a" => 1, "b" => 5];
$r = array_diff_assoc($a, $b);
foreach ($r as $k => $v) { echo "$k=$v;"; }
"#,
    );
    assert_eq!(out, "b=2;c=3;");
}

/// Verifies array_diff_assoc() compares values by PHP string cast: integer 5 and string "5"
/// are equal, so the matching pair is dropped.
/// Fixture: {x:5} vs {x:"5"} → empty result (count 0).
#[test]
fn test_array_diff_assoc_string_cast_equality() {
    let out = compile_and_run(
        r#"<?php
$r = array_diff_assoc(["x" => 5], ["x" => "5"]);
echo count($r);
"#,
    );
    assert_eq!(out, "0");
}

/// Verifies array_intersect_assoc() keeps only entries whose (key, value) pair appears in
/// the second array.
/// Fixture: {a:1,b:2,c:3} vs {a:1,b:5} → only a=1 matches.
#[test]
fn test_array_intersect_assoc_basic() {
    let out = compile_and_run(
        r#"<?php
$a = ["a" => 1, "b" => 2, "c" => 3];
$b = ["a" => 1, "b" => 5];
$r = array_intersect_assoc($a, $b);
foreach ($r as $k => $v) { echo "$k=$v;"; }
"#,
    );
    assert_eq!(out, "a=1;");
}

/// Verifies array_intersect_assoc() over string values keeps matching key+value pairs and
/// drops differing ones; exercises string-value retain plus temporary-box release.
/// Fixture: {name:"alice",role:"user"} vs {name:"alice",role:"admin"} → only name kept.
#[test]
fn test_array_intersect_assoc_string_values() {
    let out = compile_and_run(
        r#"<?php
$a = ["name" => "alice", "role" => "user"];
$b = ["name" => "alice", "role" => "admin"];
$r = array_intersect_assoc($a, $b);
foreach ($r as $k => $v) { echo "$k=$v;"; }
"#,
    );
    assert_eq!(out, "name=alice;");
}

/// Verifies array_diff_assoc() and array_intersect_assoc() are callable case-insensitively.
/// Fixture: mixed-case spellings over the same fixtures.
#[test]
fn test_assoc_diff_intersect_case_insensitive() {
    let out = compile_and_run(
        r#"<?php
$d = Array_Diff_Assoc(["a" => 1, "b" => 2], ["a" => 1]);
echo count($d);
$i = Array_Intersect_Assoc(["a" => 1, "b" => 2], ["a" => 1]);
echo count($i);
"#,
    );
    assert_eq!(out, "11");
}

/// Verifies array_replace_recursive() merges nested associative arrays key-by-key instead
/// of overwriting them wholesale.
/// Fixture: {cfg:{x:1,y:2}} replaced by {cfg:{y:9,z:3}} → cfg = {x:1,y:9,z:3}.
#[test]
fn test_array_replace_recursive_nested_merge() {
    let out = compile_and_run(
        r#"<?php
$base = ["cfg" => ["x" => 1, "y" => 2]];
$over = ["cfg" => ["y" => 9, "z" => 3]];
$r = array_replace_recursive($base, $over);
$c = $r["cfg"];
echo $c["x"];
echo $c["y"];
echo $c["z"];
"#,
    );
    assert_eq!(out, "193");
}

/// Verifies recursive replacement merges nested indexed arrays by their numeric keys.
#[test]
fn test_array_replace_recursive_nested_indexed_merge() {
    let out = compile_and_run(
        r#"<?php
$base = ["cfg" => [1, 2]];
$over = ["cfg" => [9]];
$result = array_replace_recursive($base, $over);
echo $result["cfg"][0], $result["cfg"][1];
"#,
    );
    assert_eq!(out, "92");
}

/// Verifies recursive replacement accepts heap-valued indexed top-level arrays.
#[test]
fn test_array_replace_recursive_string_indexed_inputs() {
    let out = compile_and_run(
        r#"<?php
$result = array_replace_recursive(["a", "keep"], ["b"]);
echo $result[0], $result[1];
"#,
    );
    assert_eq!(out, "bkeep");
}

/// Verifies recursive replacement descends into arrays crossing a gradual Mixed boundary.
#[test]
fn test_array_replace_recursive_nested_mixed_inputs() {
    let out = compile_and_run(
        r#"<?php
function replace_nested(mixed $base, mixed $over): mixed {
    return array_replace_recursive($base, $over);
}
$result = replace_nested(["cfg" => [1, 2]], ["cfg" => [9]]);
echo $result["cfg"][0], $result["cfg"][1];
"#,
    );
    assert_eq!(out, "92");
}

/// Verifies array_replace_recursive() overwrites non-array values like array_replace.
/// Fixture: {a:1,b:2} replaced by {b:9} → a kept, b overwritten.
#[test]
fn test_array_replace_recursive_scalar_overwrite() {
    let out = compile_and_run(
        r#"<?php
$r = array_replace_recursive(["a" => 1, "b" => 2], ["b" => 9]);
echo $r["a"];
echo $r["b"];
"#,
    );
    assert_eq!(out, "19");
}

/// Verifies array_replace_recursive() leaves the source arrays (and their nested arrays)
/// unchanged (copy-on-write through the recursive clone).
/// Fixture: nested {cfg:{x:1}} replaced by {cfg:{x:5}}; base nested x stays 1.
#[test]
fn test_array_replace_recursive_source_unchanged() {
    let out = compile_and_run(
        r#"<?php
$base = ["cfg" => ["x" => 1]];
$over = ["cfg" => ["x" => 5]];
$r = array_replace_recursive($base, $over);
$bc = $base["cfg"];
$rc = $r["cfg"];
echo $bc["x"];
echo $rc["x"];
"#,
    );
    assert_eq!(out, "15");
}

/// Verifies array_merge_recursive() recursively merges nested associative arrays sharing a key.
/// Fixture: {cfg:{a:1}} + {cfg:{b:2}} → cfg = {a:1,b:2}.
#[test]
fn test_array_merge_recursive_nested_merge() {
    let out = compile_and_run(
        r#"<?php
$r = array_merge_recursive(["cfg" => ["a" => 1]], ["cfg" => ["b" => 2]]);
foreach ($r["cfg"] as $k => $v) { echo "$k=$v;"; }
"#,
    );
    assert_eq!(out, "a=1;b=2;");
}

/// Verifies array_merge_recursive() combines two scalar values at a colliding string key
/// into a renumbered list.
/// Fixture: {k:1} + {k:2} → k = [0=>1, 1=>2].
#[test]
fn test_array_merge_recursive_scalar_combine() {
    let out = compile_and_run(
        r#"<?php
$r = array_merge_recursive(["k" => 1], ["k" => 2]);
echo count($r["k"]);
echo ":";
foreach ($r["k"] as $k => $v) { echo "$k=$v;"; }
"#,
    );
    assert_eq!(out, "2:0=1;1=2;");
}

/// Verifies array_merge_recursive() keeps non-colliding string keys from both arrays.
/// Fixture: {a:1} + {b:2} → {a:1, b:2}.
#[test]
fn test_array_merge_recursive_no_collision() {
    let out = compile_and_run(
        r#"<?php
$r = array_merge_recursive(["a" => 1], ["b" => 2]);
foreach ($r as $k => $v) { echo "$k=$v;"; }
"#,
    );
    assert_eq!(out, "a=1;b=2;");
}

/// Verifies array_merge_recursive() renumbers integer keys sequentially across both arrays.
/// Fixture: {5:10} + {9:20} → {0:10, 1:20}.
#[test]
fn test_array_merge_recursive_int_keys_renumber() {
    let out = compile_and_run(
        r#"<?php
$r = array_merge_recursive([5 => 10], [9 => 20]);
foreach ($r as $k => $v) { echo "$k=$v;"; }
"#,
    );
    assert_eq!(out, "0=10;1=20;");
}

/// Verifies array_merge_recursive() string-scalar collisions combine into a list with the
/// correct string values preserved (persisted independently of the temporary wrappers).
/// Fixture: {k:"ab"} + {k:"cd"} → k = ["ab", "cd"].
#[test]
fn test_array_merge_recursive_string_combine() {
    let out = compile_and_run(
        r#"<?php
$r = array_merge_recursive(["k" => "ab"], ["k" => "cd"]);
echo count($r["k"]);
echo ":";
foreach ($r["k"] as $v) { echo $v; echo ","; }
"#,
    );
    assert_eq!(out, "2:ab,cd,");
}

/// Verifies array_merge_recursive() is callable case-insensitively, matching PHP builtin name rules.
/// Fixture: mixed-case spelling combining two scalar values.
#[test]
fn test_array_merge_recursive_case_insensitive() {
    let out = compile_and_run(
        r#"<?php
$r = Array_Merge_Recursive(["k" => 1], ["k" => 2]);
echo count($r["k"]);
"#,
    );
    assert_eq!(out, "2");
}

/// Verifies array_multisort() sorts the first array ascending and reorders the second in tandem,
/// mutating both arrays in place (by reference).
/// Fixture: [3,1,2] + [30,10,20] → [1,2,3] + [10,20,30].
#[test]
fn test_array_multisort_parallel() {
    let out = compile_and_run(
        r#"<?php
$a = [3, 1, 2];
$b = [30, 10, 20];
array_multisort($a, $b);
foreach ($a as $v) { echo $v; }
echo "|";
foreach ($b as $v) { echo $v; }
"#,
    );
    assert_eq!(out, "123|102030");
}

/// Verifies array_multisort() returns true and leaves an already-sorted pair unchanged.
/// Fixture: [1,2,3] + [7,8,9] stay aligned; the call returns true.
#[test]
fn test_array_multisort_already_sorted_returns_true() {
    let out = compile_and_run(
        r#"<?php
$a = [1, 2, 3];
$b = [7, 8, 9];
$ok = array_multisort($a, $b);
echo $ok ? "y" : "n";
foreach ($b as $v) { echo $v; }
"#,
    );
    assert_eq!(out, "y789");
}

/// Verifies array_multisort() is callable case-insensitively and keeps a stable tandem order.
/// Fixture: [2,2,1] + [20,21,10] → first array sorted, ties keep insertion order.
#[test]
fn test_array_multisort_case_insensitive_stable() {
    let out = compile_and_run(
        r#"<?php
$a = [2, 2, 1];
$b = [20, 21, 10];
Array_Multisort($a, $b);
foreach ($a as $v) { echo $v; }
echo "|";
foreach ($b as $v) { echo $v; }
"#,
    );
    assert_eq!(out, "122|102021");
}

/// Verifies array_replace() accepts scalar indexed-array inputs: the indexed first array is
/// converted to an integer-keyed hash and the second array overwrites the matching index.
/// Fixture: [10,20,30] replaced by {1:99} → 0:10 1:99 2:30.
#[test]
fn test_array_replace_indexed_inputs() {
    let out = compile_and_run(
        r#"<?php
$r = array_replace([10, 20, 30], [1 => 99]);
foreach ($r as $k => $v) { echo "$k:$v "; }
"#,
    );
    assert_eq!(out, "0:10 1:99 2:30 ");
}

/// Verifies array_replace() with an indexed first array and an associative second array that
/// shares the integer key space (homogeneous keys) overwrites and appends correctly.
/// Fixture: [1,2,3] replaced by {1:99, 3:40} → 0:1 1:99 2:3 3:40.
#[test]
fn test_array_replace_indexed_then_int_keys() {
    let out = compile_and_run(
        r#"<?php
$r = array_replace([1, 2, 3], [1 => 99, 3 => 40]);
foreach ($r as $k => $v) { echo "$k:$v "; }
"#,
    );
    assert_eq!(out, "0:1 1:99 2:3 3:40 ");
}

/// Verifies array_replace_recursive() accepts scalar indexed inputs (converted to hashes).
/// Fixture: [1,2,3] replaced by {1:9} → 0:1 1:9 2:3.
#[test]
fn test_array_replace_recursive_indexed_inputs() {
    let out = compile_and_run(
        r#"<?php
$r = array_replace_recursive([1, 2, 3], [1 => 9]);
foreach ($r as $k => $v) { echo "$k:$v "; }
"#,
    );
    assert_eq!(out, "0:1 1:9 2:3 ");
}

/// Verifies array_diff_assoc() accepts indexed inputs, keeping first-array entries whose
/// (key, value) pair is absent from the second array.
/// Fixture: [1,2,3] minus [1,5] → 1:2 2:3 (index 0 matches value 1, index 1 differs).
#[test]
fn test_array_diff_assoc_indexed_inputs() {
    let out = compile_and_run(
        r#"<?php
$d = array_diff_assoc([1, 2, 3], [1, 5]);
foreach ($d as $k => $v) { echo "$k:$v "; }
"#,
    );
    assert_eq!(out, "1:2 2:3 ");
}

/// Verifies array_intersect_assoc() accepts indexed inputs, keeping first-array entries whose
/// (key, value) pair is present in the second array.
/// Fixture: [1,2,3] intersect [1,5] → 0:1 (only index 0 matches by key and value).
#[test]
fn test_array_intersect_assoc_indexed_inputs() {
    let out = compile_and_run(
        r#"<?php
$i = array_intersect_assoc([1, 2, 3], [1, 5]);
foreach ($i as $k => $v) { echo "$k:$v "; }
"#,
    );
    assert_eq!(out, "0:1 ");
}

/// Verifies array_merge_recursive() accepts indexed inputs, appending and renumbering integer keys.
/// Fixture: [1,2] merged with [3,4] → 0:1 1:2 2:3 3:4.
#[test]
fn test_array_merge_recursive_indexed_inputs() {
    let out = compile_and_run(
        r#"<?php
$m = array_merge_recursive([1, 2], [3, 4]);
foreach ($m as $k => $v) { echo "$k:$v "; }
"#,
    );
    assert_eq!(out, "0:1 1:2 2:3 3:4 ");
}

/// Verifies an indexed operand converted to a temporary hash is the one released after the call.
/// On x86_64 the release once loaded the temporary into `rdi` while `__rt_decref_hash` reads
/// `rax`, which still held the result: the result was freed under the caller (a reused block
/// made the next read fault) and the temporary leaked.
#[test]
fn test_two_hash_builtins_release_converted_temporaries_not_the_result() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$r = array_replace([10, 20, 30], [1 => 99]);
$m = array_merge_recursive([1, 2], [3, 4]);
$x = array_replace(["k" => 1], [5, 6]);
foreach ($r as $k => $v) { echo "$k:$v "; }
echo "| ";
foreach ($m as $k => $v) { echo "$k:$v "; }
echo "| ";
foreach ($x as $k => $v) { echo "$k:$v "; }
echo "\n";
"#,
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "0:10 1:99 2:30 | 0:1 1:2 2:3 3:4 | k:1 0:5 1:6 \n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Issue #1738: the array helpers that retain heap-backed elements must not leak when the
/// retained element outlives an overwrite, reverse, or uniqueness filter. `array_unique`
/// (indexed, nested-array elements), `array_reverse($a, true)`, `array_replace`, and
/// `array_replace_recursive` all take the retain path (runtime value tags 4..7) and must
/// leave a clean heap.
///
/// `array_merge_recursive` and the assoc diff/intersect family are deliberately excluded:
/// they already leak independently of the retain register (a pre-existing ownership bug
/// outside this change, reproduced with correct registers on aarch64 and on `origin/main`),
/// so a clean-heap assertion there would fail for a reason unrelated to #1738. The exact
/// x86_64 register pin lives in
/// `codegen_support::runtime::arrays::incref_register_tests`.
#[test]
fn test_retaining_array_helpers_leave_a_clean_heap() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$u = array_unique([[1], [2], [1]]);
unset($u);
$rev = array_reverse([[1], [2]], true);
unset($rev);
$rep = array_replace(["a" => [1]], ["a" => [2], "b" => [3]]);
unset($rep);
$repr = array_replace_recursive(["n" => ["a" => [1]]], ["n" => ["b" => [2]]]);
unset($repr);
echo "done\n";
"#,
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "done\n");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}",
        out.stderr
    );
}

/// Verifies array_merge_recursive() accepts PHP string-element indexed arrays.
#[test]
fn test_array_merge_recursive_string_indexed_inputs() {
    let out = compile_and_run(
        r#"<?php
$merged = array_merge_recursive(['a'], ['b']);
echo implode(',', $merged);
"#,
    );
    assert_eq!(out, "a,b");
}

/// Spreading an untyped method's result into a literal that is associative because of another
/// spread: `[...$parser->getWarnings(), ...$this->warnings]` is how `egulias/email-validator`
/// collects warnings. The `mixed` source was handed to `ArrayToHash`, which needs a static array
/// layout, and the build stopped with an EIR validation error. It now goes through the runtime
/// iterator; integer keys are renumbered and string keys kept. Expected output is php 8.5's.
#[test]
fn test_mixed_method_result_spreads_into_an_associative_literal() {
    let out = compile_and_run(
        r#"<?php
class P { private $w = []; public function __construct($x) { $this->w = $x; } public function getWarnings() { return $this->w; } }
class Q {
    private array $warnings = ["k" => "v"];
    public function run($p) { $this->warnings = [...$p->getWarnings(), ...$this->warnings]; return $this->warnings; }
}
$p = new P($argc > 0 ? [7 => "a", "s" => "b"] : []);
foreach ((new Q)->run($p) as $key => $value) { echo $key, "=", $value, ";"; }
"#,
    );
    assert_eq!(out, "0=a;s=b;k=v;");
}
