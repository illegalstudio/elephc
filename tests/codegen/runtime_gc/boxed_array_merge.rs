//! Purpose:
//! Verifies merges of boxed PHP arrays across storage layouts, ownership boundaries, and eval calls.
//!
//! Called from:
//! - The runtime GC codegen integration suite on every executable target.
//!
//! Key details:
//! - PHP array parameters prevent literal-only fixtures from bypassing boxed layout dispatch.
//! - Results must preserve string-key order and acquire independent owners without mutating sources.

use crate::support::*;

/// Opaque callable invocation unpacks both arrays and preserves input/result owners on return and throw.
#[test]
fn test_core_php_array_merge_descriptor_unpack_and_arity_cleanup() {
    let source = r#"<?php
function opaqueMergeInvocation(callable $callback, array $arguments): mixed {
    return call_user_func_array($callback, $arguments);
}
$merge = array_merge(...);
$left = ["shared" => str_repeat("a", 24), 4 => "left"];
$right = ["shared" => str_repeat("b", 24), 8 => "right"];
for ($i = 0; $i < 6; $i++) {
    $result = opaqueMergeInvocation($merge, [$left, $right]);
    echo implode(",", array_keys($result)), ":", strlen($result["shared"]),
        ":", $result[0], ":", $result[1], "|";
    unset($result);
}
try { opaqueMergeInvocation($merge, []); }
catch (ArgumentCountError $error) { echo "zero|"; }
unset($error);
try { opaqueMergeInvocation($merge, [$left]); }
catch (ArgumentCountError $error) { echo "one|"; }
unset($error);
try { opaqueMergeInvocation($merge, [$left, $right, []]); }
catch (ArgumentCountError $error) { echo "three|"; }
unset($error);
try { opaqueMergeInvocation($merge, [$left, 42]); }
catch (TypeError $error) { echo "type|"; }
unset($error);
echo strlen($left["shared"]), ":", strlen($right["shared"]);
unset($left, $right, $merge);
"#;
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout,
        format!("{}zero|one|three|type|24:24", "shared,0,1:24:left:right|".repeat(6)),
        "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Every packed/hash source combination renumbers integers and overwrites string keys in place.
#[test]
fn test_core_php_array_merge_layouts_keys_and_empty_sources() {
    let source = r#"<?php
function describePhpArrayMerge(array $left, array $right): void {
    $leftKeys = implode(",", array_keys($left));
    $rightKeys = implode(",", array_keys($right));
    $result = array_merge($left, $right);
    echo implode(",", array_keys($result)), ":", implode(",", $result), "|";
    if ($leftKeys !== implode(",", array_keys($left))) { echo "changed-left"; }
    if ($rightKeys !== implode(",", array_keys($right))) { echo "changed-right"; }
}
describePhpArrayMerge([10, 20], [30]);
describePhpArrayMerge(["a", "b"], ["key" => "c", 8 => "d"]);
describePhpArrayMerge(["key" => "a", 8 => "b"], ["c", "d"]);
describePhpArrayMerge([8 => "a", "same" => "old", -2 => "b"], ["same" => "new", 4 => "c", "tail" => "d"]);
describePhpArrayMerge([], []);
describePhpArrayMerge([], ["same" => "new"]);
describePhpArrayMerge(["same" => "old"], []);
"#;
    assert_eq!(compile_and_run(source),
        "0,1,2:10,20,30|0,1,key,2:a,b,c,d|key,0,1,2:a,b,c,d|0,same,1,2,tail:a,new,b,c,d|:|same:new|same:old|");
}

/// A boxed source can precede raw paired strings or follow a raw hash without changing either ABI.
#[test]
fn test_core_php_array_merge_concrete_counterparts_and_callables() {
    let source = r#"<?php
function appendConcretePhpArray(array $items): array {
    return array_merge($items, ["tail-a", "tail-b"]);
}
function prependConcretePhpArray(array $items): array {
    return array_merge(["head" => 7, 20 => 8], $items);
}
function callablePhpArrayMerge(array $items): void {
    $callback = array_merge(...);
    $first = $callback($items, ["tail"]);
    $second = call_user_func("array_merge", ["head"], $items);
    echo implode(",", $first), "|", implode(",", $second), "|";
}
$appended = appendConcretePhpArray(["named" => "first", 9 => "next"]);
echo implode(",", array_keys($appended)), ":", implode(",", $appended), "|";
$prepended = prependConcretePhpArray(["head" => 9, 4 => 10]);
echo implode(",", array_keys($prepended)), ":", implode(",", $prepended), "|";
callablePhpArrayMerge(["middle"]);
"#;
    assert_eq!(compile_and_run(source),
        "named,0,1,2:first,next,tail-a,tail-b|head,0,1:9,8,10|middle,tail|head,middle|");
}

/// Merged cells keep nested COW values and object identity alive after both sources are released.
#[test]
fn test_core_php_array_merge_retains_nested_values_and_objects() {
    let source = r#"<?php
class PhpArrayMergeObject {
    public string $name = "kept";
    public function __destruct() { echo "drop"; }
}
function mergeOwnedPhpArrays(array $left, array $right): array { return array_merge($left, $right); }
$left = ["shared" => ["value" => "first"], 5 => new PhpArrayMergeObject()];
$right = ["shared" => ["value" => "second"], "nullable" => null, "floating" => 1.5];
$result = mergeOwnedPhpArrays($left, $right);
$result["shared"]["value"] = "changed";
echo $left["shared"]["value"], ":", $right["shared"]["value"], ":", $result["shared"]["value"], "|";
echo $result["nullable"] === null ? "null" : "bad", ":", $result["floating"], "|";
unset($left, $right);
echo $result[0]->name, "|";
unset($result);
"#;
    assert_eq!(compile_and_run(source), "first:second:changed|null:1.5|kept|drop");
}

/// Merge copies do not advance source cursors, including when both operands share one container.
#[test]
fn test_core_php_array_merge_keeps_source_cursors_and_aliases() {
    let source = r#"<?php
function mergeRepeatedPhpArray(array $items): void {
    next($items);
    $result = array_merge($items, $items);
    echo key($items), ":", current($items), "|";
    echo implode(",", array_keys($result)), ":", implode(",", $result), "|";
}
mergeRepeatedPhpArray(["first", "second"]);
mergeRepeatedPhpArray([4 => "first", "same" => "second"]);
"#;
    assert_eq!(compile_and_run(source),
        "1:second|0,1,2,3:first,second,first,second|same:second|0,same,1:first,second,first|");
}

/// Opaque eval passes sparse hashes through native merge and exercises hash growth and overwrites.
#[test]
fn test_core_eval_sparse_native_php_array_merge_round_trip() {
    let source = r#"<?php
class EvalPhpArrayMerge {
    public function merge(array $left, array $right): array { return array_merge($left, $right); }
}
$code = '$object = new EvalPhpArrayMerge();
$items = ["zero", "removed", "two"];
unset($items[1]);
$result = $object->merge($items, ["key" => "value", 9 => "nine"]);
echo implode(",", array_keys($result)), ":", implode(",", $result), "|";
echo implode(",", array_keys($items)), ":", implode(",", $items), "|";
$left = ["same" => "before"];
$right = ["same" => "after"];
for ($i = 0; $i < 24; $i++) { $left[$i * 2] = $i; $right[$i * 3] = $i + 100; }
$grown = $object->merge($left, $right);
echo count($grown), ":", $grown["same"], ":", $grown[47], ":", count($left), ":", count($right);' . ' // ' . $argc;
eval($code);
"#;
    assert_eq!(compile_and_run(source), "0,1,key,2:zero,two,value,nine|0,2:zero,two|49:after:123:25:25");
}

/// Repeated overwrites release old result cells and all fresh hash/key owners without losing inputs.
#[test]
fn test_core_php_array_merge_result_owners_are_balanced() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function mergePhpArrayOwners(array $left, array $right): array { return array_merge($left, $right); }
$left = ["shared" => str_repeat("x", 24), 8 => ["value" => "left"]];
$right = ["shared" => str_repeat("y", 24), -4 => ["value" => "right"]];
for ($i = 0; $i < 40; $i++) {
    $result = mergePhpArrayOwners($left, $right);
    if (count($result) !== 3) { echo "bad"; }
    unset($result);
}
echo $left[8]["value"], ":", $right[-4]["value"];
unset($left, $right);
"#);
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "left:right", "stderr: {}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
