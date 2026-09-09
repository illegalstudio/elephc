//! Purpose:
//! Verifies boxed eval arrays retain absent numeric keys through mutation and COW.
//!
//! Called from:
//! - The runtime GC codegen integration suite.
//!
//! Key details:
//! - Removing an interior element must not recreate it as a dense null slot.
//! - Opaque eval exercises the native boxed-array bridge on every executable target.

use crate::support::*;

/// Native declared arrays retain sparse keys on removal while value copies keep their old contents.
#[test]
fn test_core_native_php_array_unset_preserves_sparse_keys_and_cow() {
    let source = r#"<?php
function removePhpArrayOffset(array $items, int $key): array {
    unset($items[$key]);
    return $items;
}
function removePhpArrayOffsetByRef(array &$items, string $key): void {
    unset($items[$key]);
}
$items = [10, 20, 30];
$removed = removePhpArrayOffset($items, 1);
echo implode(",", array_keys($removed)), ":", implode(",", $removed), "|";
echo implode(",", array_keys($items)), ":", implode(",", $items), "|";
$hash = ["keep" => 41, "drop" => 42];
$snapshot = $hash;
removePhpArrayOffsetByRef($hash, "drop");
echo implode(",", array_keys($hash)), ":", implode(",", $hash), "|";
echo implode(",", array_keys($snapshot)), ":", implode(",", $snapshot);
"#;
    assert_eq!(compile_and_run(source), "0,2:10,30|0,1,2:10,20,30|keep:41|keep,drop:41,42");
}

/// An owned computed key and the detached box are rooted while a removed element throws.
#[test]
fn test_core_native_php_array_unset_throw_preserves_reference_owner() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class ThrowingPhpArrayElement {
    public function __destruct() { throw new RuntimeException("removed"); }
}
function removeThrowingPhpArrayElement(array &$items): void {
    unset($items[str_repeat("d", 8)]);
}
$items = ["dddddddd" => new ThrowingPhpArrayElement(), "keep" => 41];
try { removeThrowingPhpArrayElement($items); }
catch (RuntimeException $error) { echo $error->getMessage(), "|"; unset($error); }
echo count($items), ":", $items["keep"], ":";
echo array_key_exists("dddddddd", $items) ? "present" : "absent";
unset($items);
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "removed|1:41:absent", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Boxed array reversal preserves source keys and string keys while obeying the runtime numeric-key policy.
#[test]
fn test_core_native_php_array_reverse_preserves_keys_and_source() {
    let source = r#"<?php
function describePhpArrayReverse(array $items, bool $preserve): void {
    $result = array_reverse($items, preserve_keys: $preserve);
    echo implode(",", array_keys($result)), ":", implode(",", $result), "|";
    echo implode(",", array_keys($items)), ":", implode(",", $items), "|";
}
describePhpArrayReverse([10, 20, 30], false);
describePhpArrayReverse(["first", "second"], true);
describePhpArrayReverse([4 => "four", "label" => "text", -2 => "negative"], false);
describePhpArrayReverse([4 => "four", "label" => "text", -2 => "negative"], true);
describePhpArrayReverse([], false);
describePhpArrayReverse([], true);
"#;
    assert_eq!(compile_and_run(source),
        "0,1,2:30,20,10|0,1,2:10,20,30|1,0:second,first|0,1:first,second|0,label,1:negative,text,four|4,label,-2:four,text,negative|-2,label,4:negative,text,four|4,label,-2:four,text,negative|:|:|:|:|");
}

/// Reversed Mixed cells own their nested arrays independently and keep object identity alive.
#[test]
fn test_core_native_php_array_reverse_retains_nested_values() {
    let source = r#"<?php
class PhpArrayReverseObject { public string $name = "kept"; }
function reversePhpArrayValues(array $items): array { return array_reverse($items); }
$original = [["key" => "original"], new PhpArrayReverseObject(), null, 1.5];
$result = reversePhpArrayValues($original);
$result[3]["key"] = "changed";
echo $original[0]["key"], ":", $result[3]["key"], "|";
echo $result[1] === null ? "null" : "bad", ":", $result[0], "|";
unset($original);
echo $result[2]->name, ":", $result[3]["key"], "|";
$packed = [["value" => "first"], ["value" => "second"]];
$reversed = reversePhpArrayValues($packed);
unset($packed);
echo $reversed[0]["value"], ":", $reversed[1]["value"];
"#;
    assert_eq!(compile_and_run(source), "original:changed|null:1.5|kept:changed|second:first");
}

/// Eval passes sparse arrays into a native reversal method and receives the complete boxed result.
#[test]
fn test_core_eval_sparse_native_array_reverse_round_trip() {
    let source = r#"<?php
class EvalPhpArrayReverse {
    public function reverse(array $items, bool $preserve): array {
        return array_reverse($items, $preserve);
    }
}
$code = '$object = new EvalPhpArrayReverse();
$items = ["zero", "removed", "two"];
unset($items[1]);
$kept = $object->reverse($items, true);
$numbered = $object->reverse($items, false);
echo implode(",", array_keys($kept)), ":", implode(",", $kept), "|";
echo implode(",", array_keys($numbered)), ":", implode(",", $numbered), "|";
echo implode(",", array_keys($items)), ":", implode(",", $items);' . ' // ' . $argc;
eval($code);
"#;
    assert_eq!(compile_and_run(source), "2,0:two,zero|0,1:two,zero|0,2:zero,two");
}

/// Repeated boxed reversals release result hashes, persisted string keys, and retained Mixed cells.
#[test]
fn test_core_native_php_array_reverse_result_owners_are_balanced() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function reverseOwnedPhpArray(array $items, bool $preserve): array {
    return array_reverse($items, $preserve);
}
$items = ["name" => str_repeat("x", 16), 8 => 23, "child" => ["value" => "kept"]];
for ($i = 0; $i < 40; $i++) {
    $result = reverseOwnedPhpArray($items, $i % 2 === 0);
    if (count($result) !== 3) { echo "bad"; }
    unset($result);
}
echo $items["child"]["value"];
unset($items);
"#);
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "kept", "stderr: {}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Nested packed slots expose one boxed array reference while preserving outer COW copies.
#[test]
fn test_core_native_php_array_element_reference_preserves_parent_copy() {
    let source = r#"<?php
function updateNestedArray(array &$items): void { $items["added"] = 7; }
$outer = [[1]];
$copy = $outer;
updateNestedArray($outer[0]);
echo implode(",", array_keys($outer[0])), ":", implode(",", $outer[0]), "|";
echo implode(",", $copy[0]), "|";
$hashes = [["kept" => "original"]];
$hashCopy = $hashes;
updateNestedArray($hashes[0]);
echo implode(",", array_keys($hashes[0])), ":", implode(",", $hashes[0]), "|";
echo implode(",", $hashCopy[0]);
"#;
    assert_eq!(compile_and_run(source), "0,added:1,7|1|kept,added:original,7|original");
}

/// Passing the same nested array twice keeps an actual shared slot instead of two writeback copies.
#[test]
fn test_core_native_php_array_element_reference_repeated_alias() {
    let source = r#"<?php
function updateAliasedArrays(array &$first, array &$second): void {
    $first["first"] = 2;
    echo count($second), ":";
    $second["second"] = 3;
    echo count($first), "|";
}
$outer = [[1]];
updateAliasedArrays($outer[0], $outer[0]);
echo implode(",", array_keys($outer[0])), ":", implode(",", $outer[0]);
"#;
    assert_eq!(compile_and_run(source), "2:3|0,first,second:1,2,3");
}

/// Native array_values accepts boxed PHP arrays and does not change source keys or shared values.
#[test]
fn test_core_native_php_array_values_preserve_source_layout() {
    let source = r#"<?php
function projectPhpArrayValues(array $items): array {
    $result = array_values($items);
    echo implode(",", array_keys($items)), ":", implode(",", $items), "|";
    return $result;
}
echo implode(",", projectPhpArrayValues([10, 20])), "|";
echo implode(",", projectPhpArrayValues(["first", "second"])), "|";
echo implode(",", projectPhpArrayValues([4 => "four", 1 => "one"])), "|";
echo implode(",", projectPhpArrayValues(["a" => 3, "b" => 7])), "|";
echo count(projectPhpArrayValues([])), "|";
function projectNestedPhpArrayValues(array $items): array { return array_values($items); }
$original = [["key" => "kept"]];
$copy = projectNestedPhpArrayValues($original);
$copy[0]["key"] = "changed";
echo $original[0]["key"], ":", $copy[0]["key"];
"#;
    assert_eq!(compile_and_run(source),
        "0,1:10,20|10,20|0,1:first,second|first,second|4,1:four,one|four,one|a,b:3,7|3,7|:|0|kept:changed");
}

/// PHP array contracts remain usable by key probes and internal-pointer operations after boxing.
#[test]
fn test_core_native_php_array_key_probes_and_cursor() {
    let source = r#"<?php
function describePhpArrayKeys(array $items): void {
    echo array_key_first($items), ":", array_key_last($items), ":";
    echo array_is_list($items) ? "list:" : "hash:";
    echo current($items), ":";
    next($items);
    echo key($items), ":", current($items), "|";
}
function observePhpArrayReference(array &$items): void { echo count($items), ":"; }
describePhpArrayKeys([10, 20]);
describePhpArrayKeys(["a" => 30, "b" => 40]);
$items = [50, 60];
next($items);
observePhpArrayReference($items);
echo key($items), ":", current($items);
"#;
    assert_eq!(compile_and_run(source), "0:1:list:10:1:20|a:b:hash:30:b:40|2:1:60");
}

/// Copies of PHP array parameters keep independent boxes during native key and append writes.
#[test]
fn test_core_native_php_array_local_writes_preserve_parameter_copies() {
    let source = r#"<?php
function changePhpArrayCopy(array $items): array {
    $copy = $items;
    $copy["key"] = "new";
    $copy[] = "tail";
    echo implode(",", $items), "|";
    return $copy;
}
$source = ["old"];
$result = changePhpArrayCopy($source);
echo implode(",", array_keys($result)), ":", implode(",", $result), "|";
echo implode(",", $source);
"#;
    assert_eq!(compile_and_run(source), "old|0,key,1:old,new,tail|old");
}

/// Native PHP array properties accept append and string keys without mutating copied values.
#[test]
fn test_core_native_php_array_property_writes_preserve_copies() {
    let source = r#"<?php
class PhpArrayPropertyWrites {
    public array $items = [1];
    public static array $shared = [2];
    public function write(): void {
        $this->items["key"] = 3;
        $this->items[] = 4;
        self::$shared["key"] = 5;
        self::$shared[] = 6;
    }
}
$owner = new PhpArrayPropertyWrites();
$copy = $owner->items;
$staticCopy = PhpArrayPropertyWrites::$shared;
$owner->write();
echo implode(",", array_keys($owner->items)), ":", implode(",", $owner->items), "|";
echo implode(",", array_keys(PhpArrayPropertyWrites::$shared)), ":", implode(",", PhpArrayPropertyWrites::$shared), "|";
echo implode(",", $copy), "|", implode(",", $staticCopy);
"#;
    assert_eq!(compile_and_run(source), "0,key,1:1,3,4|0,key,1:2,5,6|1|2");
}

/// Bare PHP array ref parameters preserve packed and hash callers through boxed storage writeback.
#[test]
fn test_core_native_php_array_reference_parameters_preserve_keys() {
    let source = r#"<?php
function appendPhpArray(array &$items): void { $items["added"] = 7; }
$packed = [1];
$hash = ["old" => 2];
echo count($packed), ":", count($hash), "|";
appendPhpArray($packed);
appendPhpArray($hash);
echo implode(",", array_keys($packed)), ":", implode(",", $packed), "|";
echo implode(",", array_keys($hash)), ":", implode(",", $hash);
"#;
    assert_eq!(compile_and_run(source), "1:1|0,added:1,7|old,added:2,7");
}

/// Eval constructor and by-reference method calls share the unrestricted PHP array ABI.
#[test]
fn test_core_eval_sparse_native_array_constructor_and_reference_parameter() {
    let source = r#"<?php
class NativeSparseArrayConstructor {
    public array $items;
    public function __construct(array $items) { $this->items = $items; }
    public function append(array &$items): void { $items[] = "tail"; }
    public function describe(): string {
        return implode(",", array_keys($this->items)) . ":" . implode(",", $this->items);
    }
}
$source = '$items = [4 => "four", 1 => "one"];
$object = new NativeSparseArrayConstructor($items);
$object->append($items);
echo $object->describe(), "|", implode(",", array_keys($items)), ":", implode(",", $items);' . ' // ' . $argc;
eval($source);
"#;
    assert_eq!(compile_and_run(source), "4,1:four,one|4,1,5:four,one,tail");
}

/// Native array parameters and returns preserve eval hash ordering and copy-on-write ownership.
#[test]
fn test_core_eval_sparse_native_array_parameters_and_returns() {
    let source = r#"<?php
class NativeSparseArrayBoundary {
    public function describe(array $items): string {
        return count($items) . ":" . implode(",", array_keys($items)) . ":" . implode(",", $items);
    }
    public function append(array $items): array {
        $items[] = "tail";
        return $items;
    }
}
$source = '$object = new NativeSparseArrayBoundary();
$items = [1 => "one", 0 => "zero"];
$copy = $items;
echo $object->describe($items), "|";
$appended = $object->append($items);
echo $object->describe($appended), "|", $object->describe($copy);' . ' // ' . $argc;
eval($source);
"#;
    assert_eq!(compile_and_run(source),
        "2:1,0:one,zero|3:1,0,2:one,zero,tail|2:1,0:one,zero");
}

/// Declared array properties retain sparse keys across eval writes and subsequent native reads.
#[test]
fn test_core_eval_sparse_declared_array_properties_preserve_native_reads() {
    let source = r#"<?php
class NativeSparseArrayProperties {
    public array $items = [10, 20, 30];
    public static array $shared = [40, 50, 60];
    public function describe(): string {
        return count($this->items) . ":" . implode(",", array_keys($this->items))
            . "|" . count(self::$shared) . ":" . implode(",", array_keys(self::$shared));
    }
}
$source = '$owner = new NativeSparseArrayProperties();
unset($owner->items[1]);
unset(NativeSparseArrayProperties::$shared[0]);
$copy = $owner->items;
$owner->items[1000000] = 70;
echo $owner->describe(), "|", count($copy), ":", implode(",", array_keys($copy));' . ' // ' . $argc;
eval($source);
"#;
    assert_eq!(compile_and_run(source), "3:0,2,1000000|2:1,2|2:0,2");
}

/// Unset retains sparse keys, distinguishes missing from null, and leaves copied arrays intact.
#[test]
fn test_core_eval_unset_preserves_sparse_keys_and_cow() {
    let source = r#"<?php
$source = '$array = [10, 20, 30]; $copy = $array;
unset($array[1]);
echo count($array), ":", implode(",", array_keys($array)), ":";
echo array_key_exists(1, $array) ? "bad" : "missing";
echo "|", implode(",", $copy), "|";
$array[1] = null;
echo count($array), ":", implode(",", array_keys($array)), ":";
echo array_key_exists(1, $array) ? "null-present" : "bad";' . ' // ' . $argc;
eval($source);
"#;
    assert_eq!(compile_and_run(source), "2:0,2:missing|10,20,30|3:0,2,1:null-present");
}

/// Sparse writes do not allocate dense gaps, including through native Mixed property writeback.
#[test]
fn test_core_eval_sparse_array_writeback_preserves_property_owners() {
    let source = r#"<?php
class SparseNativeOwner { public mixed $items = null; }
$source = '$owner = new SparseNativeOwner();
$owner->items = [10, 20, 30];
unset($owner->items[1]);
$copy = $owner->items;
$owner->items[1000000] = 40;
echo count($owner->items), ":", implode(",", array_keys($owner->items)), "|";
echo count($copy), ":", implode(",", array_keys($copy));
unset($owner, $copy);' . ' // ' . $argc;
eval($source);
"#;
    assert_eq!(compile_and_run(source), "3:0,2,1000000|2:0,2");
}
