//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of indexed array sorting, including asort, arsort, and ksort.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

/// Verifies `array_unique()` accepts an argument that is ALREADY a hash.
///
/// It did not compile at all — `unsupported EIR backend feature: array_unique for PHP type
/// AssocArray` — which turned ordinary PHP into a build failure. The indexed builder cannot serve
/// this case: it walks fixed-size slots, and a hash has none.
///
/// Duplicates are found through a second hash keyed by the VALUE rather than by rescanning what
/// has been emitted, so the walk stays linear and equal strings collapse without a byte scan —
/// hash keys already compare by value. The two `"pomme"` are built by separate concatenations to
/// pin that: keyed by pointer they would both survive.
///
/// Each survivor keeps its ORIGINAL key, which is the whole reason `array_unique` returns a hash
/// in the first place, so the keys are what the assertion checks.
#[test]
fn test_array_unique_accepts_a_hash_and_keeps_the_first_key() {
    let out = compile_and_run(
        r#"<?php
var_dump(array_unique([7 => 30, 2 => 10, 9 => 30, 4 => 20, 1 => 10]));
var_dump(array_unique(["c" => "pear", "a" => "apple", "z" => "pear", "b" => "fig"]));
$x = "po" . "mme";
$y = "pom" . "me";
var_dump(array_unique(["k1" => $x, "k2" => $y, "k3" => "kiwi"]));
echo count(array_unique(["a" => 1, "b" => 1, "c" => 1])), "\n";
"#,
    );
    assert_eq!(
        out,
        "array(3) {\n  [7]=>\n  int(30)\n  [2]=>\n  int(10)\n  [4]=>\n  int(20)\n}\n\
         array(3) {\n  [\"c\"]=>\n  string(4) \"pear\"\n  [\"a\"]=>\n  string(5) \"apple\"\n  \
         [\"b\"]=>\n  string(3) \"fig\"\n}\n\
         array(2) {\n  [\"k1\"]=>\n  string(5) \"pomme\"\n  [\"k3\"]=>\n  string(4) \"kiwi\"\n}\n1\n"
    );
}

/// Verifies `array_unique()` accepts a STRING list and compares its elements by value.
///
/// The call did not compile at all: the shared 8-byte element gate refuses `Str`, because a string
/// slot is a 16-byte (pointer, length) pair. That gate is also `array_reverse`, `shuffle` and
/// `array_merge`, none of which compare their elements, so it stays as it is and `array_unique`
/// reads the element type itself.
///
/// The two `"pomme"` below are built by separate concatenations on purpose. They are equal
/// strings at DIFFERENT addresses, so the word-equality the scan used for integers would have
/// kept both — the same defect that still makes boxed elements a refusal rather than an answer.
/// PHP compares by string rendering, which for a string array is byte equality.
///
/// Keys are asserted through `var_dump` and `json_encode`: `array_unique` keeps each survivor's
/// ORIGINAL key, so the result is sparse and encodes as a JSON object, not a list.
#[test]
fn test_array_unique_deduplicates_a_string_list_by_value() {
    let out = compile_and_run(
        r#"<?php
$u = array_unique(["pear", "apple", "pear", "fig", "apple"]);
var_dump($u);
echo json_encode($u), "\n";
$x = "po" . "mme";
$y = "pom" . "me";
var_dump(array_unique([$x, $y, "kiwi"]));
echo count(array_unique(["a", "a", "a"])), "\n";
"#,
    );
    assert_eq!(
        out,
        "array(3) {\n  [0]=>\n  string(4) \"pear\"\n  [1]=>\n  string(5) \"apple\"\n  \
         [3]=>\n  string(3) \"fig\"\n}\n{\"0\":\"pear\",\"1\":\"apple\",\"3\":\"fig\"}\n\
         array(2) {\n  [0]=>\n  string(5) \"pomme\"\n  [2]=>\n  string(4) \"kiwi\"\n}\n1\n"
    );
}

/// Verifies `sort()` and `rsort()` accept a HASH receiver and renumber it from zero.
///
/// `sort(array_unique($xs))` is everyday PHP — `array_unique` preserves the original keys, so its
/// result is a hash the moment anything was dropped — and the backend used to refuse the whole
/// program with `unsupported EIR backend feature: sort for PHP type AssocArray`. That refusal was
/// the only thing keeping this branch's CI red, on all three architectures.
///
/// The result is asserted through its SHAPE, not just its order: `sort()` discards keys, so what
/// distinguishes a correct result from a merely value-sorted one is that the keys come back as
/// `0..n-1`. `var_dump` and `json_encode` are the two readers that show it — a hash whose keys are
/// not dense would encode as an object instead of a list.
#[test]
fn test_sort_and_rsort_renumber_a_hash_receiver_from_zero() {
    let out = compile_and_run(
        r#"<?php
$u = array_unique([30, 10, 30, 20, 10]);
sort($u);
var_dump($u);
echo json_encode($u), "\n";
foreach ($u as $k => $v) { echo $k, "=>", $v, " "; }
echo "\n", count($u), ":", $u[0], ":", $u[2], "\n";

$s = ["c" => "pear", "a" => "apple", "b" => "fig"];
sort($s);
echo implode(",", $s), "\n";

$r = array_unique([1, 3, 1, 2]);
rsort($r);
echo implode(",", $r), "\n";
"#,
    );
    assert_eq!(
        out,
        "array(3) {\n  [0]=>\n  int(10)\n  [1]=>\n  int(20)\n  [2]=>\n  int(30)\n}\n\
         [10,20,30]\n0=>10 1=>20 2=>30 \n3:10:30\napple,fig,pear\n3,2,1\n"
    );
}

/// Verifies the reverse-sorted fast path reverses once for both `sort()` and `rsort()`.
#[test]
fn test_sort_and_rsort_reverse_sorted_fast_path() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$a = range(100, 1);
sort($a);
echo implode(',', $a), '|';
$b = range(1, 100);
rsort($b);
echo implode(',', $b), '|';
$c = [5, 5, 3, 3, 1];
sort($c);
echo implode(',', $c), '|';
$d = [-3, -1, -2, 0];
sort($d);
echo implode(',', $d), "\n";
"#,
    );
    let ascending = (1..=100).map(|n| n.to_string()).collect::<Vec<_>>().join(",");
    let descending = (1..=100).rev().map(|n| n.to_string()).collect::<Vec<_>>().join(",");
    assert!(out.success, "program crashed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        format!("{ascending}|{descending}|1,3,3,5,5|-3,-2,-1,0\n")
    );
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}",
        out.stderr
    );
}

/// Verifies asort keeps each value on its original key.
/// Fixture: [3, 1, 2] sorts to keys 1, 2, 0, so `$a[0]` stays 3.
#[test]
fn test_asort() {
    let out = compile_and_run(
        r#"<?php
$a = [3, 1, 2];
asort($a);
echo $a[0];
"#,
    );
    assert_eq!(out, "3");
}

/// Verifies arsort keeps each value on its original key.
/// Fixture: [1, 3, 2] sorts to keys 1, 2, 0, so `$a[0]` stays 1.
#[test]
fn test_arsort() {
    let out = compile_and_run(
        r#"<?php
$a = [1, 3, 2];
arsort($a);
echo $a[0];
"#,
    );
    assert_eq!(out, "1");
}

/// Verifies ksort sorts by keys in ascending order, preserving values.
/// Fixture: [3, 1, 2] with string keys → sorted by key → count remains 3.
#[test]
fn test_ksort() {
    let out = compile_and_run(
        r#"<?php
$a = [3, 1, 2];
ksort($a);
echo count($a);
"#,
    );
    assert_eq!(out, "3");
}

/// Verifies krsort sorts by keys in descending order, preserving key/value association.
/// An indexed array stores its keys as slot positions, so the receiver must be a hash;
/// fixture: [2 => 1, 1 => 2, 3 => 3] → descending key order 3, 2, 1 with values 3, 1, 2.
#[test]
fn test_krsort_assoc_preserves_key_value_pairs() {
    let out = compile_and_run(
        r#"<?php
$a = [2 => 1, 1 => 2, 3 => 3];
krsort($a);
echo count($a);
foreach ($a as $k => $v) { echo ":", $k, "=", $v; }
"#,
    );
    assert_eq!(out, "3:3=3:2=1:1=2");
}

/// Verifies `krsort()` reverses indexed-key iteration order while preserving each key/value pair.
#[test]
fn test_krsort_indexed_reverses_key_iteration() {
    let out = compile_and_run(
        r#"<?php
$a = [3, 1, 2];
krsort($a);
foreach ($a as $key => $value) {
    echo $key . ":" . $value . ",";
}
"#,
    );
    assert_eq!(out, "2:2,1:1,0:3,");
}

/// Verifies `krsort()` publishes packed-to-hash promotion through an array-element reference.
#[test]
fn test_krsort_mutates_packed_array_element_reference() {
    let out = compile_and_run(
        r#"<?php
$matrix = ["row" => [3, 1, 2]];
$original = $matrix;
krsort($matrix["row"]);
foreach ($matrix["row"] as $key => $value) {
    echo $key . ":" . $value . ",";
}
echo "|";
foreach ($original["row"] as $key => $value) {
    echo $key . ":" . $value . ",";
}
"#,
    );
    assert_eq!(out, "2:2,1:1,0:3,|0:3,1:1,2:2,");
}

/// Verifies `krsort()` publishes packed-to-hash promotion through a typed property reference.
#[test]
fn test_krsort_mutates_packed_typed_property_reference() {
    let out = compile_and_run(
        r#"<?php
class SortFixture {
    public array $values = [3, 1, 2];
}

$fixture = new SortFixture();
krsort($fixture->values);
foreach ($fixture->values as $key => $value) {
    echo $key . ":" . $value . ",";
}
"#,
    );
    assert_eq!(out, "2:2,1:1,0:3,");
}

/// Verifies `krsort()` preserves a typed property's representation and performs
/// copy-on-write when the property's packed value came from a non-literal assignment.
#[test]
fn test_krsort_cow_splits_non_literal_typed_property_assignment() {
    let out = compile_and_run(
        r#"<?php
class KrsortPropertyFixture {
    public array $items = [];
}

$fixture = new KrsortPropertyFixture();
$source = [3, 1, 2];
$fixture->items = $source;
$alias = $fixture->items;
krsort($fixture->items);
foreach ($fixture->items as $key => $value) {
    echo $key . ":" . $value . ",";
}
echo "|";
foreach ($alias as $key => $value) {
    echo $key . ":" . $value . ",";
}
"#,
    );
    assert_eq!(out, "2:2,1:1,0:3,|0:3,1:1,2:2,");
}

/// Verifies exact `array` parameters, static properties, and stabilized property receivers share
/// boxed-cell key sorting without exposing mutations through an earlier PHP array copy.
#[test]
fn test_key_sorts_exact_declared_array_places_across_storage_shapes() {
    let out = compile_and_run(
        r#"<?php
function printReverseArrayKeys(array $values): void {
    krsort($values);
    foreach ($values as $key => $value) { echo $key, ":", $value, ","; }
}
function printMaybeReverseArrayKeys(array $values, bool $reverse): void {
    if ($reverse) { krsort($values); }
    foreach ($values as $key => $value) { echo $key, ":", $value, ","; }
}
function returnArrayAlias(array $values): array {
    return $values;
}
function reverseArrayKeysByReference(array &$values): void {
    krsort($values);
}

class ExactKeySortParameterOwner {
    public function __construct(public array $items) {}
}
$parameterOwner = new ExactKeySortParameterOwner([3, 1, 2]);
$parameterAlias = $parameterOwner->items;
printReverseArrayKeys($parameterOwner->items);
echo "|";
foreach ($parameterOwner->items as $key => $value) { echo $key, ":", $value, ","; }
echo "|";
foreach ($parameterAlias as $key => $value) { echo $key, ":", $value, ","; }
echo "|";
printMaybeReverseArrayKeys($parameterOwner->items, false);
echo "|";
$returned = returnArrayAlias($parameterOwner->items);
krsort($returned);
foreach ($returned as $key => $value) { echo $key, ":", $value, ","; }
echo "|";
foreach ($parameterOwner->items as $key => $value) { echo $key, ":", $value, ","; }
echo "|";
foreach ($parameterAlias as $key => $value) { echo $key, ":", $value, ","; }
echo "|";
$byRefOwner = new ExactKeySortParameterOwner([3, 1, 2]);
$byRefAlias = $byRefOwner->items;
$byRefValues = $byRefOwner->items;
reverseArrayKeysByReference($byRefValues);
foreach ($byRefValues as $key => $value) { echo $key, ":", $value, ","; }
echo "|";
foreach ($byRefOwner->items as $key => $value) { echo $key, ":", $value, ","; }
echo "|";
foreach ($byRefAlias as $key => $value) { echo $key, ":", $value, ","; }
echo "|";

class ExactKeySortStatic {
    public static array $items = ["b" => 2, "a" => 1];
}
$staticAlias = ExactKeySortStatic::$items;
ksort(ExactKeySortStatic::$items);
foreach (ExactKeySortStatic::$items as $key => $value) { echo $key, $value; }
echo "|";
foreach ($staticAlias as $key => $value) { echo $key, $value; }
echo "|";

class ExactKeySortOwner {
    public function __construct(public array $items) {}
}
$receiverCalls = 0;
function selectExactKeySortOwner(ExactKeySortOwner $owner): ExactKeySortOwner {
    global $receiverCalls;
    $receiverCalls++;
    return $owner;
}
$owner = new ExactKeySortOwner([3, 1, 2]);
$propertyAlias = $owner->items;
krsort(selectExactKeySortOwner($owner)->items);
echo $receiverCalls, "|";
foreach ($owner->items as $key => $value) { echo $key, ":", $value, ","; }
echo "|";
foreach ($propertyAlias as $key => $value) { echo $key, ":", $value, ","; }
"#,
    );
    assert_eq!(
        out,
        "2:2,1:1,0:3,|0:3,1:1,2:2,|0:3,1:1,2:2,|0:3,1:1,2:2,|2:2,1:1,0:3,|0:3,1:1,2:2,|0:3,1:1,2:2,|2:2,1:1,0:3,|0:3,1:1,2:2,|0:3,1:1,2:2,|a1b2|b2a1|1|2:2,1:1,0:3,|0:3,1:1,2:2,"
    );
}

/// Verifies named `array:` arguments preserve packed-key promotion for locals and nested lvalues.
#[test]
fn test_krsort_named_array_argument_promotes_packed_lvalues() {
    let out = compile_and_run(
        r#"<?php
$local = [3, 1, 2];
krsort(array: $local);
foreach ($local as $key => $value) {
    echo $key . ":" . $value . ",";
}
echo "|";
$matrix = ["row" => [3, 1, 2]];
krsort(array: $matrix["row"]);
foreach ($matrix["row"] as $key => $value) {
    echo $key . ":" . $value . ",";
}
"#,
    );
    assert_eq!(out, "2:2,1:1,0:3,|2:2,1:1,0:3,");
}

/// Verifies repeated `krsort()` calls reuse an already-promoted nested hash safely.
#[test]
fn test_krsort_reuses_promoted_nested_element() {
    let out = compile_and_run(
        r#"<?php
$matrix = ["row" => [3, 1, 2]];
for ($i = 0; $i < 2; $i++) {
    krsort($matrix["row"]);
}
foreach ($matrix["row"] as $key => $value) {
    echo $key . ":" . $value . ",";
}
"#,
    );
    assert_eq!(out, "2:2,1:1,0:3,");
}

/// Verifies `krsort()` accepts a nested element already stored as an associative hash.
#[test]
fn test_krsort_accepts_nested_element_already_stored_as_hash() {
    let out = compile_and_run(
        r#"<?php
$matrix = ["row" => [3, 1, 2, "x" => 4]];
krsort($matrix["row"]);
foreach ($matrix["row"] as $key => $value) {
    echo $key . ":" . $value . ",";
}
"#,
    );
    assert_eq!(out, "x:4,2:2,1:1,0:3,");
}

/// Verifies nested associative hashes are COW-split and written back before key sorting.
#[test]
fn test_krsort_writes_back_shared_nested_associative_hash() {
    let out = compile_and_run(
        r#"<?php
$matrix = ["row" => ["a" => 1, "b" => 2]];
$original = $matrix;
krsort($matrix["row"]);
foreach ($matrix["row"] as $key => $value) {
    echo $key . ":" . $value . ",";
}
echo "|";
foreach ($original["row"] as $key => $value) {
    echo $key . ":" . $value . ",";
}
"#,
    );
    assert_eq!(out, "b:2,a:1,|a:1,b:2,");
}

/// Verifies reverse key sorting promotes and writes back an element of a packed parent.
#[test]
fn test_krsort_mutates_packed_parent_element_reference() {
    let out = compile_and_run(
        r#"<?php
$grid = [[3, 1, 2], [9, 8, 7]];
$original = $grid;
krsort($grid[0]);
foreach ($grid[0] as $key => $value) {
    echo $key . ":" . $value . ",";
}
echo "|";
foreach ($original[0] as $key => $value) {
    echo $key . ":" . $value . ",";
}
"#,
    );
    assert_eq!(out, "2:2,1:1,0:3,|0:3,1:1,2:2,");
}

/// Verifies distinct nested key-sort statements reuse a widened packed parent.
#[test]
fn test_krsort_reuses_widened_packed_parent_across_statements() {
    let out = compile_and_run(
        r#"<?php
$grid = [[3, 1, 2], [9, 8, 7]];
krsort($grid[0]);
krsort($grid[1]);
foreach ($grid[0] as $key => $value) {
    echo $key . ":" . $value . ",";
}
echo "|";
foreach ($grid[1] as $key => $value) {
    echo $key . ":" . $value . ",";
}
"#,
    );
    assert_eq!(out, "2:2,1:1,0:3,|2:7,1:8,0:9,");
}

/// Verifies nested key sorting works when the packed parent starts with Mixed cells.
#[test]
fn test_krsort_nested_element_of_initially_mixed_packed_parent() {
    let out = compile_and_run(
        r#"<?php
$grid = [[3, 1, 2], "sentinel"];
krsort($grid[0]);
foreach ($grid[0] as $key => $value) {
    echo $key . ":" . $value . ",";
}
echo "|" . $grid[1];
"#,
    );
    assert_eq!(out, "2:2,1:1,0:3,|sentinel");
}

/// Verifies a born-Mixed packed parent detaches before its nested cell is promoted.
#[test]
fn test_krsort_cow_splits_initially_mixed_packed_parent() {
    let out = compile_and_run(
        r#"<?php
$grid = [[3, 1, 2], "sentinel"];
$original = $grid;
krsort($grid[0]);
foreach ($grid[0] as $key => $value) {
    echo $key . ":" . $value . ",";
}
echo "|";
foreach ($original[0] as $key => $value) {
    echo $key . ":" . $value . ",";
}
"#,
    );
    assert_eq!(out, "2:2,1:1,0:3,|0:3,1:1,2:2,");
}

/// Verifies an already-widened associative parent detaches before sorting a sibling cell.
#[test]
fn test_krsort_cow_splits_widened_associative_parent() {
    let out = compile_and_run(
        r#"<?php
$matrix = ["left" => [3, 1, 2], "right" => [9, 8, 7]];
krsort($matrix["left"]);
$original = $matrix;
krsort($matrix["right"]);
foreach ($matrix["right"] as $key => $value) {
    echo $key . ":" . $value . ",";
}
echo "|";
foreach ($original["right"] as $key => $value) {
    echo $key . ":" . $value . ",";
}
"#,
    );
    assert_eq!(out, "2:7,1:8,0:9,|0:9,1:8,2:7,");
}

/// Verifies an integer-typed dynamic index reaches the nested Mixed-cell lowering path.
#[test]
fn test_krsort_nested_mixed_parent_with_dynamic_integer_index() {
    let out = compile_and_run(
        r#"<?php
$grid = [[3, 1, 2], "sentinel"];
$index = 0;
krsort($grid[$index]);
foreach ($grid[0] as $key => $value) {
    echo $key . ":" . $value . ",";
}
"#,
    );
    assert_eq!(out, "2:2,1:1,0:3,");
}

/// Verifies a scalar Mixed child remains a controlled krsort type error.
#[test]
fn test_krsort_scalar_child_of_mixed_packed_parent_reports_type_error() {
    let out = compile_and_run_capture(
        r#"<?php
$grid = [[3, 1, 2], "sentinel"];
krsort($grid[1]);
"#,
    );
    assert!(!out.success, "scalar nested value should fail");
    assert!(
        out.stdout.contains("krsort()")
            && out.stdout.contains("Argument #1")
            && out.stdout.contains("array"),
        "expected a controlled krsort array TypeError, got: {}",
        out.stdout,
    );
}

/// Verifies a missing Mixed child remains a controlled krsort type error.
#[test]
fn test_krsort_missing_child_of_mixed_packed_parent_reports_type_error() {
    let out = compile_and_run_capture(
        r#"<?php
$grid = [[3, 1, 2], "sentinel"];
krsort($grid[9]);
"#,
    );
    assert!(!out.success, "missing nested value should fail");
    assert!(
        out.stdout.contains("krsort()")
            && out.stdout.contains("Argument #1")
            && out.stdout.contains("array"),
        "expected a controlled krsort array TypeError, got: {}",
        out.stdout,
    );
}

/// Verifies separate nested key-sort statements reuse an already-promoted child hash.
#[test]
fn test_krsort_reuses_promoted_nested_element_across_statements() {
    let out = compile_and_run(
        r#"<?php
$matrix = ["row" => [3, 1, 2]];
krsort($matrix["row"]);
krsort($matrix["row"]);
foreach ($matrix["row"] as $key => $value) {
    echo $key . ":" . $value . ",";
}
"#,
    );
    assert_eq!(out, "2:2,1:1,0:3,");
}

/// Verifies a missing nested element fails with a PHP-level type error instead of crashing.
#[test]
fn test_krsort_missing_nested_element_reports_type_error() {
    let out = compile_and_run_capture(
        r#"<?php
$matrix = ["row" => [3, 1, 2]];
krsort($matrix["missing"]);
"#,
    );
    assert!(!out.success, "missing nested array should fail");
    assert!(
        out.stdout.contains("krsort()")
            && out.stdout.contains("Argument #1")
            && out.stdout.contains("array"),
        "expected a controlled krsort array TypeError, got: {}",
        out.stdout,
    );
}

/// Verifies natsort keeps each integer on its original key.
/// Fixture: [3, 1, 2] sorts to keys 1, 2, 0, so `$a[0]` stays 3.
#[test]
fn test_natsort() {
    let out = compile_and_run(
        r#"<?php
$a = [3, 1, 2];
natsort($a);
echo $a[0];
"#,
    );
    assert_eq!(out, "3");
}

/// Verifies natcasesort keeps each integer on its original key.
/// Fixture: [3, 1, 2] sorts to keys 1, 2, 0, so `$a[0]` stays 3.
#[test]
fn test_natcasesort() {
    let out = compile_and_run(
        r#"<?php
$a = [3, 1, 2];
natcasesort($a);
echo $a[0];
"#,
    );
    assert_eq!(out, "3");
}

/// Verifies key-preserving sorts on a packed list keep the original keys.
///
/// `asort`, `arsort`, `uasort`, `uksort`, `natsort`, and `natcasesort` used to
/// permute the packed slots and renumber them from zero.
#[test]
fn test_packed_list_key_preserving_sorts() {
    let out = compile_and_run(
        r#"<?php
function pairs(array $items): string {
    $out = "";
    foreach ($items as $key => $value) {
        $out .= $key . "=" . $value . " ";
    }
    return $out;
}
$a = [3, 1, 2];
asort($a);
echo pairs($a), "\n";
$a = [3, 1, 2];
arsort($a);
echo pairs($a), "\n";
$a = [3, 1, 2];
uasort($a, fn($x, $y) => $x <=> $y);
echo pairs($a), "\n";
$a = [3, 1, 2];
uksort($a, fn($x, $y) => $y <=> $x);
echo pairs($a), "\n";
$a = [3, 1, 2];
natsort($a);
echo pairs($a), "\n";
$a = [3, 1, 2];
natcasesort($a);
echo pairs($a), "\n";
"#,
    );
    assert_eq!(
        out,
        "1=1 2=2 0=3 \n0=3 2=2 1=1 \n1=1 2=2 0=3 \n2=2 1=1 0=3 \n1=1 2=2 0=3 \n1=1 2=2 0=3 \n"
    );
}

/// Verifies a sort of the original local still keeps keys when another name aliases it.
///
/// `unset` of that alias must leave the sorted list intact, and a later same-typed binding
/// of the unset name must not be retagged as the promoted hash.
#[test]
fn test_aliased_packed_list_sort_keeps_both_names() {
    let out = compile_and_run(
        r#"<?php
$a = [3, 1, 2];
$r = &$a;
asort($a);
foreach ($a as $key => $value) { echo $key, "=", $value, " "; }
echo "\n";
foreach ($r as $key => $value) { echo $key, "=", $value, " "; }
echo "\n";
$b = [3, 1, 2];
$s = &$b;
krsort($b);
foreach ($b as $key => $value) { echo $key, "=", $value, " "; }
echo "\n";
$c = [3, 1, 2];
$t = &$c;
unset($t);
$t = [9, 8];
asort($c);
foreach ($t as $key => $value) { echo $key, "=", $value, " "; }
echo "\n";
foreach ($c as $key => $value) { echo $key, "=", $value, " "; }
echo "\n";
$d = [3, 1, 2];
$u = &$d;
unset($u);
krsort($d);
echo isset($u) ? "yes" : "no", "\n";
"#,
    );
    assert_eq!(
        out,
        "1=1 2=2 0=3 \n1=1 2=2 0=3 \n2=2 1=1 0=3 \n0=9 1=8 \n1=1 2=2 0=3 \nno\n"
    );
}

/// Verifies a conditional sort of an aliased packed list keeps both names on the hash layout.
///
/// The branch is taken (`$argc > 0`). An untaken loop must leave the original list readable
/// instead of treating the packed storage as the hash the body would have built.
#[test]
fn test_conditional_aliased_packed_list_sort_keeps_keys() {
    let out = compile_and_run(
        r#"<?php
$a = [3, 1, 2];
$r = &$a;
if ($argc > 0) { asort($a); }
foreach ($a as $key => $value) { echo $key, "=", $value, " "; }
echo "\n";
foreach ($r as $key => $value) { echo $key, "=", $value, " "; }
echo "\n";
$b = [3, 1, 2];
$s = &$b;
if ($argc > 0) { asort($s); }
foreach ($b as $key => $value) { echo $key, "=", $value, " "; }
echo "\n";
$c = [3, 1, 2];
$t = &$c;
$n = 0;
while ($n < 0) { asort($c); }
foreach ($c as $key => $value) { echo $key, "=", $value, " "; }
echo "\n";
$d = [3, 1, 2];
$u = &$d;
$n = 0;
while ($n < 1) { asort($d); $n = $n + 1; }
foreach ($d as $key => $value) { echo $key, "=", $value, " "; }
"#,
    );
    assert_eq!(
        out,
        "1=1 2=2 0=3 \n1=1 2=2 0=3 \n1=1 2=2 0=3 \n0=3 1=1 2=2 \n1=1 2=2 0=3 "
    );
}

/// Verifies builtins that follow a key-preserving sort still see the list's keys.
///
/// The sort promotes the packed list. `array_walk`, `array_reverse`, `array_splice`, and
/// `array_unshift` must keep working, including when the sort is in a branch that does not run.
#[test]
fn test_packed_list_after_key_sort_accepts_followup_builtins() {
    let out = compile_and_run(
        r#"<?php
$a = [30, 10];
asort($a);
array_walk($a, function ($value, $key) { echo $key, "=", $value, " "; });
echo "\n";
$b = [30, 10];
asort($b);
$rev = array_reverse($b);
echo $rev[0], ",", $rev[1], "\n";
$c = [30, 10];
asort($c);
$removed = array_splice($c, 0, 1);
echo $removed[0], "\n";
foreach ($c as $key => $value) { echo $key, "=", $value, " "; }
echo "\n";
$d = [30, 10];
asort($d);
array_unshift($d, 9);
foreach ($d as $key => $value) { echo $key, "=", $value, " "; }
echo "\n";
$e = [30, 10];
if ($argc < 0) { asort($e); }
array_walk($e, function ($value, $key) { echo $key, "=", $value, " "; });
"#,
    );
    assert_eq!(out, "1=10 0=30 \n30,10\n10\n0=30 \n0=9 1=10 2=30 \n0=30 1=10 ");
}

/// Verifies argument evaluation, spreads, and object lifetimes after a key-preserving sort.
#[test]
fn test_sorted_hash_followups_keep_argument_order_and_destructors() {
    let out = compile_and_run(
        r#"<?php
class D {
    function __destruct() { echo "d"; }
}
function f() {
    $a = [new D(), new D()];
    asort($a);
    array_reverse($a);
}
f();
echo "x\n";
$c = [30, 10];
asort($c);
function off(array $items) {
    foreach ($items as $key => $value) { echo $key, "=", $value, " "; }
    return 0;
}
array_splice($c, off($c), 1);
echo "\n";
$b = [30, 10];
asort($b);
$rev = array_reverse($b, ...[false]);
echo $rev[0], ",", $rev[1], "\n";
"#,
    );
    assert_eq!(out, "ddx\n1=10 0=30 \n30,10\n");
}

/// Verifies splice and by-reference walk release objects, and that a by-reference
/// argument can append to the sorted list before splice renumbers it.
#[test]
fn test_sorted_list_splice_and_walk_release_objects() {
    let out = compile_and_run(
        r#"<?php
class D {
    public int $id;
    function __construct(int $id) { $this->id = $id; }
    function __destruct() { echo "d", $this->id; }
}
function splice_objects() {
    $a = [new D(1), new D(2)];
    asort($a);
    $removed = array_splice($a, 0, 1);
    echo $removed[0]->id, " ", $a[0]->id, "|";
}
splice_objects();
echo "\n";
function poke_splice() {
    $c = [30, 10];
    asort($c);
    $poke = function (array &$items): int { $items[] = 7; return 0; };
    array_splice($c, $poke($c), 1);
    foreach ($c as $key => $value) { echo $key, "=", $value, " "; }
}
poke_splice();
echo "\n";
function walk_objects() {
    $a = [new D(1), new D(2)];
    array_walk($a, function (&$v, $k) { echo $k, ":", $v->id, " "; });
    echo "|";
}
walk_objects();
"#,
    );
    assert_eq!(out, "1 2|d2d1\n0=30 1=7 \n0:1 1:2 |d1d2");
}

/// Verifies list builtins that discard keys still accept a list after a key-preserving sort.
#[test]
fn test_key_discarding_builtins_accept_a_sorted_packed_list() {
    let out = compile_and_run(
        r#"<?php
$a = [30, 10, 20];
asort($a);
usort($a, fn($x, $y) => $x <=> $y);
echo $a[0], ",", $a[1], ",", $a[2], "\n";
$b = [30, 10, 20];
asort($b);
$slice = array_slice($b, 0, 1);
echo $slice[0], "\n";
$kept = array_slice($b, 0, 2, true);
echo $kept[1], ",", $kept[2], "\n";
$c = [30, 10, 20];
asort($c);
$padded = array_pad($c, 4, 7);
echo $padded[0], ",", $padded[1], ",", $padded[2], ",", $padded[3], "\n";
"#,
    );
    assert_eq!(out, "10,20,30\n10\n10,20\n10,20,30,7\n");
}

/// Verifies a variadic walk receives the key and a variable by-reference callback writes it back.
#[test]
fn test_array_walk_variadic_and_variable_callback() {
    let out = compile_and_run(
        r#"<?php
function walk_all(...$args) { echo count($args); }
function walk_ref(&$value, $key) { $value = $key; }
function walk_one($value) { echo $value, " "; }
$a = [30, 10];
array_walk($a, "walk_all");
echo "\n";
$f = "walk_ref";
$b = [30, 10];
array_walk($b, $f);
echo $b[0], ",", $b[1], "\n";
$g = "walk_one";
$c = [30, 10];
array_walk($c, $g);
"#,
    );
    assert_eq!(out, "22\n0,1\n30 10 ");
}

/// Verifies `krsort` through a reference sorts the packed list and leaves an earlier copy intact.
#[test]
fn test_krsort_reference_to_packed_list() {
    let out = compile_and_run(
        r#"<?php
$a = [3, 1, 2];
$r = &$a;
krsort($r);
$out = "";
foreach ($a as $key => $value) {
    $out .= $key . "=" . $value . " ";
}
echo $out, "\n";
$b = [3, 1, 2];
$s = &$b;
$copy = $b;
krsort($s);
echo $copy[0], $copy[1], $copy[2], "\n";
foreach ($b as $key => $value) {
    echo $key, "=", $value, " ";
}
"#,
    );
    assert_eq!(out, "2=2 1=1 0=3 \n312\n2=2 1=1 0=3 ");
}

/// Verifies `ksort` under string flags reorders decimal keys of a packed list.
///
/// `SORT_NATURAL` and the default leave `0..n-1` in place. `SORT_STRING` and
/// `SORT_LOCALE_STRING` compare the decimal text, so `10` sorts before `2`.
#[test]
fn test_ksort_packed_list_string_flags() {
    let out = compile_and_run(
        r#"<?php
function keys(array $items): string {
    $out = "";
    foreach ($items as $key => $value) {
        $out .= $key . " ";
    }
    return $out;
}
$a = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11];
ksort($a, SORT_STRING);
echo keys($a), "\n";
$b = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11];
ksort($b, SORT_NATURAL);
echo keys($b), "\n";
$c = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11];
ksort($c);
echo keys($c), "\n";
$d = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11];
ksort($d, SORT_LOCALE_STRING);
echo keys($d), "\n";
"#,
    );
    assert_eq!(
        out,
        "0 1 10 11 2 3 4 5 6 7 8 9 \n0 1 2 3 4 5 6 7 8 9 10 11 \n0 1 2 3 4 5 6 7 8 9 10 11 \n0 1 10 11 2 3 4 5 6 7 8 9 \n"
    );
}

/// Verifies `array_walk` passes the key of a local packed list for closures, arrows, and named callbacks.
#[test]
fn test_array_walk_packed_list_passes_key() {
    let out = compile_and_run(
        r#"<?php
function show($value, $key) { echo $key, "=", $value, " "; }
function show_one($value) { echo $value, " "; }
function walk_local() {
    $items = [30, 10];
    array_walk($items, function ($value, $key) { echo $key, "=", $value, " "; });
}
$a = [30, 10];
array_walk($a, function ($value, $key) { echo $key, "=", $value, " "; });
echo "\n";
$b = [30, 10];
array_walk($b, function (&$value, $key) { $value = $key; });
echo $b[0], ",", $b[1], "\n";
$c = [30, 10];
array_walk($c, fn($value, $key) => print($key . "=" . $value . " "));
echo "\n";
$d = [30, 10];
array_walk($d, "show");
echo "\n";
$e = [30, 10];
array_walk($e, "show_one");
echo "\n";
$f = [30, 10];
array_walk($f, function (int $value, int $key) { echo $key, "=", $value, " "; });
echo "\n";
walk_local();
"#,
    );
    assert_eq!(
        out,
        "0=30 1=10 \n0,1\n0=30 1=10 \n0=30 1=10 \n30 10 \n0=30 1=10 \n0=30 1=10 "
    );
}

/// Verifies compiled PHP output for sort string array.
#[test]
fn test_sort_string_array() {
    let out = compile_and_run(
        r#"<?php
$a = ["banana", "apple", "cherry", "date"];
sort($a);
echo $a[0] . "," . $a[1] . "," . $a[2] . "," . $a[3];
"#,
    );
    assert_eq!(out, "apple,banana,cherry,date");
}

/// Verifies `usort` still shows the sorted keys to a callback argument, then renumbers.
#[test]
fn test_usort_callback_sees_keys_before_reindex() {
    let out = compile_and_run(
        r#"<?php
function dump_keys($items) {
    foreach ($items as $key => $_) { echo $key; }
    return "cmp";
}
function cmp($left, $right) { return $left <=> $right; }
$a = [30, 10, 20];
asort($a);
usort($a, dump_keys($a));
echo "\n", $a[0], ",", $a[1], ",", $a[2];
"#,
    );
    assert_eq!(out, "120\n10,20,30");
}

/// Verifies a method callable array writes a by-reference element back, and a by-value method does not.
#[test]
fn test_array_walk_method_array_writes_by_ref_element() {
    let out = compile_and_run(
        r#"<?php
class Walker {
    static function write(&$value, $key) { $value = $key; }
    static function localOnly($value, $key) { $value = $key; }
    function writeInstance(&$value, $key) { $value = $key; }
}
$a = [30, 10];
array_walk($a, ["Walker", "write"]);
echo $a[0], ",", $a[1], "\n";
$b = [30, 10];
array_walk($b, ["Walker", "localOnly"]);
echo $b[0], ",", $b[1], "\n";
$walker = new Walker();
$c = [30, 10];
array_walk($c, [$walker, "writeInstance"]);
echo $c[0], ",", $c[1];
"#,
    );
    assert_eq!(out, "0,1\n30,10\n0,1");
}

/// Verifies unsetting an alias inside a try or a loop does not resurrect that alias afterwards.
///
/// A later binding of the unset name is its own list. Sorting the original local must not
/// retag that new list. A try that leaves the alias in place still sorts both names together.
#[test]
fn test_unset_alias_inside_try_or_loop_stays_detached() {
    let out = compile_and_run(
        r#"<?php
function show($arr) {
    foreach ($arr as $key => $value) { echo $key, "=", $value, " "; }
    echo "\n";
}
$a = [3, 1, 2];
$r = &$a;
try { unset($r); } catch (Exception $err) {}
$r = [9, 8];
asort($a);
show($r);
$b = [3, 1, 2];
$s = &$b;
$n = 0;
while ($n < 1) { unset($s); $n++; }
$s = [9, 8];
asort($b);
show($s);
$c = [3, 1, 2];
$t = &$c;
try { unset($t); } finally {}
$t = [9, 8];
asort($c);
show($t);
$d = [3, 1, 2];
$u = &$d;
$n = 0;
do { unset($u); $n++; } while ($n < 1);
$u = [9, 8];
asort($d);
show($u);
$kept = [3, 1, 2];
$alias = &$kept;
for ($n = 0; $n < 1; $n++) { unset($alias); }
$alias = [9, 8];
asort($kept);
show($alias);
$f = [3, 1, 2];
$w = &$f;
foreach ([1] as $_) { unset($w); }
$w = [9, 8];
asort($f);
show($w);
$g = [3, 1, 2];
$x = &$g;
try { echo "t"; } catch (Exception $err) { echo "c"; }
krsort($x);
echo " ";
show($g);
"#,
    );
    assert_eq!(
        out,
        "0=9 1=8 \n0=9 1=8 \n0=9 1=8 \n0=9 1=8 \n0=9 1=8 \n0=9 1=8 \nt 2=2 1=1 0=3 \n"
    );
}

/// Verifies a by-reference walk callback is found through case and first-class syntax.
#[test]
fn test_array_walk_by_ref_callback_ignores_case_and_accepts_first_class() {
    let out = compile_and_run(
        r#"<?php
function walk_ref(&$value, $key) { $value = $key; }
function walk_value($value, $key) { echo $key, "=", $value, " "; }
$a = [30, 10];
array_walk($a, "WALK_REF");
echo $a[0], ",", $a[1], "\n";
$b = [30, 10];
array_walk($b, walk_ref(...));
echo $b[0], ",", $b[1], "\n";
$c = [30, 10];
array_walk($c, "WALK_VALUE");
echo "\n", $c[0], ",", $c[1];
"#,
    );
    assert_eq!(out, "0,1\n0,1\n0=30 1=10 \n30,10");
}

/// Verifies compiled PHP output for rsort string array.
#[test]
fn test_rsort_string_array() {
    let out = compile_and_run(
        r#"<?php
$a = ["banana", "apple", "cherry", "date"];
rsort($a);
echo $a[0] . "," . $a[1] . "," . $a[2] . "," . $a[3];
"#,
    );
    assert_eq!(out, "date,cherry,banana,apple");
}
