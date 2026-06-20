//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of array literals, indexing, and string offsets, including literal and count, access, and access variable index.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use crate::support::*;

// --- Arrays ---

/// Compiles `[1, 2, 3]` and verifies `count()` returns the array length.
#[test]
fn test_array_literal_and_count() {
    let out = compile_and_run("<?php $a = [1, 2, 3]; echo count($a);");
    assert_eq!(out, "3");
}

/// Compiles `[10, 20, 30]` and accesses elements at literal indices 0, 1, 2.
#[test]
fn test_array_access() {
    let out =
        compile_and_run("<?php $a = [10, 20, 30]; echo $a[0] . \" \" . $a[1] . \" \" . $a[2];");
    assert_eq!(out, "10 20 30");
}

/// Verifies array access variable index.
#[test]
fn test_array_access_variable_index() {
    let out = compile_and_run("<?php $a = [10, 20, 30]; $i = 2; echo $a[$i];");
    assert_eq!(out, "30");
}

/// Verifies string indexing returns single character.
#[test]
fn test_string_indexing_returns_single_character() {
    let out = compile_and_run(r#"<?php $s = "hello"; echo $s[1];"#);
    assert_eq!(out, "e");
}

/// Verifies string indexing out of bounds returns empty string.
#[test]
fn test_string_indexing_out_of_bounds_returns_empty_string() {
    let out = compile_and_run(r#"<?php $s = "hello"; echo "[" . $s[99] . "]";"#);
    assert_eq!(out, "[]");
}

/// Verifies string indexing negative offset counts from end.
#[test]
fn test_string_indexing_negative_offset_counts_from_end() {
    let out = compile_and_run(r#"<?php $s = "hello"; echo $s[-1];"#);
    assert_eq!(out, "o");
}

/// Verifies string indexing with variable offset.
#[test]
fn test_string_indexing_with_variable_offset() {
    let out = compile_and_run(r#"<?php $s = "hello"; $i = 3; echo $s[$i];"#);
    assert_eq!(out, "l");
}

/// Verifies string indexing accepts numeric string offsets.
#[test]
fn test_string_indexing_accepts_numeric_string_offsets() {
    let out = compile_and_run(
        r#"<?php $s = "abcd"; echo $s["0"]; echo $s["01"]; echo $s["+2"]; echo $s[" -1 "]; echo "\n"; echo isset($s["3"]) ? "y" : "n"; echo isset($s["4"]) ? "y\n" : "n\n";"#,
    );
    assert_eq!(out, "abcd\nyn\n");
}

/// Verifies string indexing empty string returns empty string.
#[test]
fn test_string_indexing_empty_string_returns_empty_string() {
    let out = compile_and_run(r#"<?php $s = ""; $i = 0; echo "[" . $s[$i] . "]";"#);
    assert_eq!(out, "[]");
}

/// Verifies string indexing negative beyond length returns empty.
#[test]
fn test_string_indexing_negative_beyond_length_returns_empty() {
    let out = compile_and_run(r#"<?php $s = "hi"; echo "[" . $s[-10] . "]";"#);
    assert_eq!(out, "[]");
}

/// Verifies string indexing exactly negative length returns first.
#[test]
fn test_string_indexing_exactly_negative_length_returns_first() {
    let out = compile_and_run(r#"<?php $s = "abc"; echo $s[-3];"#);
    assert_eq!(out, "a");
}

/// Verifies string indexing at length returns empty.
#[test]
fn test_string_indexing_at_length_returns_empty() {
    let out = compile_and_run(r#"<?php $s = "ab"; echo "[" . $s[2] . "]";"#);
    assert_eq!(out, "[]");
}

/// Verifies string indexing last valid index.
#[test]
fn test_string_indexing_last_valid_index() {
    let out = compile_and_run(r#"<?php $s = "abc"; echo $s[2];"#);
    assert_eq!(out, "c");
}

/// Verifies array assign.
#[test]
fn test_array_assign() {
    let out = compile_and_run("<?php $a = [1, 2, 3]; $a[1] = 99; echo $a[1];");
    assert_eq!(out, "99");
}

/// Verifies array compound assign.
#[test]
fn test_array_compound_assign() {
    let out = compile_and_run("<?php $a = [1, 2, 3]; $a[1] += 40; $a[2] *= 10; echo $a[1] . \"|\" . $a[2];");
    assert_eq!(out, "42|30");
}

/// Verifies array compound assign evaluates index once.
#[test]
fn test_array_compound_assign_evaluates_index_once() {
    let out = compile_and_run(
        r#"<?php
function idx() {
    echo "i";
    return 1;
}

$a = [10, 20, 30];
$a[idx()] += 5;
echo ":" . $a[1];
"#,
    );
    assert_eq!(out, "i:25");
}

/// Verifies array compound assign effectful index all operator families.
#[test]
fn test_array_compound_assign_effectful_index_all_operator_families() {
    let out = compile_and_run(
        r#"<?php
function idx() {
    echo ".";
    return 0;
}

$num = [2];
$num[idx()] **= 3;
echo ":" . $num[0];

$bits = [8];
$bits[idx()] >>= 1;
echo ":" . $bits[0];

$text = ["a"];
$text[idx()] .= "b";
echo ":" . $text[0];

$fallback = [null];
$fallback[idx()] ??= 7;
echo ":" . $fallback[0];
"#,
    );
    assert_eq!(out, ".:8.:4.:ab.:7");
}

/// Verifies array assign into empty array updates length.
#[test]
fn test_array_assign_into_empty_array_updates_length() {
    let out = compile_and_run(r#"<?php $a = []; $a[0] = 7; echo count($a) . "|" . $a[0];"#);
    assert_eq!(out, "1|7");
}

/// Verifies array push.
#[test]
fn test_array_push() {
    let out = compile_and_run("<?php $a = [1, 2]; $a[] = 3; echo count($a) . \" \" . $a[2];");
    assert_eq!(out, "3 3");
}

/// Verifies array push builtin.
#[test]
fn test_array_push_builtin() {
    let out =
        compile_and_run("<?php $a = [10]; array_push($a, 20); echo count($a) . \" \" . $a[1];");
    assert_eq!(out, "2 20");
}

/// Regression: appending strings into an empty `[]` literal past its initial capacity must
/// not corrupt the first element. An empty literal is typed `array<never>`; `array_new`
/// previously sized its slots at 8 bytes, but the first string append specializes the header
/// to 16-byte `{ptr,len}` slots in place without reallocating, overflowing the undersized
/// backing store. Growth then copied the overflowed bytes and the first element came out
/// garbled. Pushing 8 strings forces at least one grow from the initial 4-element capacity.
#[test]
fn test_empty_array_string_append_grows() {
    let out = compile_and_run(
        r#"<?php
$a = [];
for ($i = 0; $i < 8; $i++) { $a[] = "x"; }
echo implode(",", $a);
"#,
    );
    assert_eq!(out, "x,x,x,x,x,x,x,x");
}

/// Regression: same empty-array grow corruption, exercised with distinct interpolated strings
/// so a mis-sized first slot is caught by value (not just by a repeated character). Verifies
/// the first element survives the grow and every appended string round-trips.
#[test]
fn test_empty_array_interpolated_string_append_grows() {
    let out = compile_and_run(
        r#"<?php
$a = [];
for ($i = 0; $i < 10; $i++) { $a[] = "item$i"; }
echo implode("|", $a);
"#,
    );
    assert_eq!(out, "item0|item1|item2|item3|item4|item5|item6|item7|item8|item9");
}

/// Regression guard: an empty `[]` literal that first receives refcounted (object) elements must
/// also survive growth. Object slots are 8-byte pointers, so the empty `array<never>` buffer is
/// already the right size and only the string-append path needs the capacity rescale — this test
/// confirms the fix left the pointer-slot grow path untouched. Pushing 6 objects forces a grow
/// from the initial 4-element capacity.
#[test]
fn test_empty_array_object_append_grows() {
    let out = compile_and_run(
        r#"<?php
class P { public function __construct(public string $n) {} }
$a = [];
for ($i = 0; $i < 6; $i++) { $a[] = new P("p$i"); }
echo $a[0]->n . "|" . $a[5]->n . "|" . count($a);
"#,
    );
    assert_eq!(out, "p0|p5|6");
}

/// Verifies array access on function call result.
#[test]
fn test_array_access_on_function_call_result() {
    let out = compile_and_run(
        r#"<?php
function getColor() {
    return [255, 128, 0];
}
echo getColor()[1];
"#,
    );
    assert_eq!(out, "128");
}

/// Verifies foreach int.
#[test]
fn test_foreach_int() {
    let out = compile_and_run("<?php $a = [1, 2, 3]; foreach ($a as $v) { echo $v; }");
    assert_eq!(out, "123");
}

/// Verifies foreach value by reference mutates indexed array.
#[test]
fn test_foreach_value_by_reference_mutates_indexed_array() {
    let out = compile_and_run(
        r#"<?php
$a = [1, 2, 3];
foreach ($a as &$v) {
    $v *= 2;
}
foreach ($a as $x) {
    echo $x;
}
"#,
    );
    assert_eq!(out, "246");
}

/// Verifies foreach value by reference reuse value name in next loop.
#[test]
fn test_foreach_value_by_reference_reuse_value_name_in_next_loop() {
    let out = compile_and_run(
        r#"<?php
$a = [1, 2, 3];
foreach ($a as $k => &$v) {
    $v *= 2;
}
foreach ($a as $k => $v) {
    echo $k . "=" . $v . ";";
}
"#,
    );
    assert_eq!(out, "0=2;1=4;2=4;");
}

/// Verifies foreach value by reference post assignment mutates last element.
#[test]
fn test_foreach_value_by_reference_post_assignment_mutates_last_element() {
    let out = compile_and_run(
        r#"<?php
$a = [1, 2, 3];
foreach ($a as &$v) {
    $v += 10;
}
$v = 99;
foreach ($a as $x) {
    echo $x;
}
echo "|" . $v;
"#,
    );
    assert_eq!(out, "111299|99");
}

/// Verifies foreach value by reference empty loop preserves existing value.
#[test]
fn test_foreach_value_by_reference_empty_loop_preserves_existing_value() {
    let out = compile_and_run(
        r#"<?php
$v = 7;
$a = [1];
array_pop($a);
foreach ($a as &$v) {
    $v = 9;
}
echo $v;
"#,
    );
    assert_eq!(out, "7");
}

/// Verifies foreach value by reference rebinds existing reference param.
#[test]
fn test_foreach_value_by_reference_rebinds_existing_reference_param() {
    let out = compile_and_run(
        r#"<?php
function update(&$v) {
    $a = [1];
    foreach ($a as &$v) {
        $v = 2;
    }
    $v = 9;
    echo $a[0] . "|" . $v;
}

$x = 5;
update($x);
echo "|" . $x;
"#,
    );
    assert_eq!(out, "9|9|5");
}

/// Verifies foreach value by reference splits COW indexed array.
#[test]
fn test_foreach_value_by_reference_splits_cow_indexed_array() {
    let out = compile_and_run(
        r#"<?php
$a = [1, 2];
$b = $a;
foreach ($b as &$v) {
    $v *= 3;
}
foreach ($a as $x) {
    echo $x;
}
echo "|";
foreach ($b as $x) {
    echo $x;
}
"#,
    );
    assert_eq!(out, "12|36");
}

/// Verifies foreach string.
#[test]
fn test_foreach_string() {
    let out = compile_and_run(r#"<?php $a = ["a", "b", "c"]; foreach ($a as $v) { echo $v; }"#);
    assert_eq!(out, "abc");
}

/// Verifies foreach break.
#[test]
fn test_foreach_break() {
    let out = compile_and_run(
        "<?php $a = [1, 2, 3, 4, 5]; foreach ($a as $v) { if ($v == 3) { break; } echo $v; }",
    );
    assert_eq!(out, "12");
}

/// Verifies array in function.
#[test]
fn test_array_in_function() {
    let out = compile_and_run(
        r#"<?php
function sum($arr) {
    $total = 0;
    foreach ($arr as $v) {
        $total += $v;
    }
    return $total;
}
echo sum([1, 2, 3, 4, 5]);
"#,
    );
    assert_eq!(out, "15");
}

/// Verifies string array.
#[test]
fn test_string_array() {
    let out = compile_and_run(
        r#"<?php
$names = ["Alice", "Bob"];
$names[] = "Charlie";
echo count($names) . ": ";
foreach ($names as $n) { echo $n . " "; }
"#,
    );
    assert_eq!(out, "3: Alice Bob Charlie ");
}

// --- Array functions ---

/// Verifies array pop.
#[test]
fn test_array_pop() {
    let out =
        compile_and_run("<?php $a = [1, 2, 3]; $v = array_pop($a); echo $v . \" \" . count($a);");
    assert_eq!(out, "3 2");
}

/// Verifies array pop empty.
#[test]
fn test_array_pop_empty() {
    let out = compile_and_run("<?php $a = [1]; array_pop($a); echo array_pop($a);");
    assert_eq!(out, "");
}

/// Verifies in array found.
#[test]
fn test_in_array_found() {
    let out = compile_and_run("<?php $a = [10, 20, 30]; echo in_array(20, $a);");
    assert_eq!(out, "1");
}

/// Verifies in array not found. `in_array` returns bool, so `echo false` is the empty
/// string (not "0").
#[test]
fn test_in_array_not_found() {
    let out = compile_and_run("<?php $a = [10, 20, 30]; echo in_array(99, $a);");
    assert_eq!(out, "");
}

/// Verifies in array string found.
#[test]
fn test_in_array_string_found() {
    let out = compile_and_run(r#"<?php $a = ["a", "b", "c"]; echo in_array("b", $a);"#);
    assert_eq!(out, "1");
}

/// Verifies in array string not found. A false result echoes as the empty string.
#[test]
fn test_in_array_string_not_found() {
    let out = compile_and_run(r#"<?php $a = ["a", "b", "c"]; echo in_array("x", $a);"#);
    assert_eq!(out, "");
}

/// Verifies `in_array` returns a real `bool` (var_dump shows bool, not int), matching PHP.
/// Regression: previously typed `int`, so `var_dump` printed `int(1)`/`int(0)` and a false
/// result echoed as "0" instead of "".
#[test]
fn test_in_array_returns_bool() {
    let out = compile_and_run(
        r#"<?php
$a = [10, 20, 30];
var_dump(in_array(20, $a));
var_dump(in_array(99, $a));
var_dump(in_array(99, $a) === false);
"#,
    );
    assert_eq!(out, "bool(true)\nbool(false)\nbool(true)\n");
}

/// Verifies sort.
#[test]
fn test_sort() {
    let out =
        compile_and_run(r#"<?php $a = [5, 3, 1, 4, 2]; sort($a); foreach ($a as $v) { echo $v; }"#);
    assert_eq!(out, "12345");
}

/// Verifies rsort.
#[test]
fn test_rsort() {
    let out =
        compile_and_run(r#"<?php $a = [1, 3, 2]; rsort($a); foreach ($a as $v) { echo $v; }"#);
    assert_eq!(out, "321");
}

/// Verifies array keys.
#[test]
fn test_array_keys() {
    let out = compile_and_run(
        r#"<?php $a = [10, 20, 30]; $k = array_keys($a); foreach ($k as $v) { echo $v; }"#,
    );
    assert_eq!(out, "012");
}

/// Verifies isset.
#[test]
fn test_isset() {
    let out = compile_and_run("<?php $x = 42; echo isset($x);");
    assert_eq!(out, "1");
}

/// Verifies isset multiple arguments requires all non null.
#[test]
fn test_isset_multiple_arguments_requires_all_non_null() {
    let out = compile_and_run(
        r#"<?php
$a = 1;
$b = null;
echo isset($a, $b) ? "yes\n" : "no\n";
"#,
    );
    assert_eq!(out, "no\n");
}

/// Verifies isset multiple arguments short circuits.
#[test]
fn test_isset_multiple_arguments_short_circuits() {
    let out = compile_and_run(
        r#"<?php
function mark(): int {
    echo "bad";
    return 0;
}
$a = null;
$items = [1];
echo isset($a, $items[mark()]) ? "yes" : "no";
"#,
    );
    assert_eq!(out, "no");
}

/// Verifies isset array element empty string and missing key.
#[test]
fn test_isset_array_element_empty_string_and_missing_key() {
    let out = compile_and_run(
        r#"<?php
$items = [""];
echo isset($items[0]);
echo isset($items[1]);
$mixed = [null, 0];
echo isset($mixed[0]);
echo isset($mixed[1]);
$map = ["name" => ""];
echo isset($map["name"]);
echo isset($map["missing"]);
"#,
    );
    // `isset` is a bool: echoing `false` yields "" (not "0"), matching PHP.
    // Set/false sequence T,F,F,T,T,F renders as "1","","","1","1","" = "111".
    assert_eq!(out, "111");
}

/// Verifies unset multiple variables.
#[test]
fn test_unset_multiple_variables() {
    let out = compile_and_run(
        r#"<?php
$a = 1;
$b = 2;
unset($a, $b);
echo isset($a) ? "a\n" : "na\n";
echo isset($b) ? "b\n" : "nb\n";
"#,
    );
    assert_eq!(out, "na\nnb\n");
}

/// Verifies `unset($hash[$key])` removes a string-keyed associative-array entry, leaving the
/// remaining entries, their iteration order, and `count()`/`isset()` consistent with PHP.
#[test]
fn test_unset_assoc_string_key() {
    let out = compile_and_run(
        r#"<?php
$m = ['a' => 1, 'b' => 2, 'c' => 3];
unset($m['b']);
echo count($m), "\n";
foreach ($m as $k => $v) { echo "$k=$v\n"; }
echo isset($m['b']) ? "has-b\n" : "no-b\n";
echo isset($m['a']) ? "has-a\n" : "no-a\n";
"#,
    );
    assert_eq!(out, "2\na=1\nc=3\nno-b\nhas-a\n");
}

/// Verifies a removed entry leaves a tombstone that keeps probe chains intact: after removing
/// a key, later inserts still resolve, and re-adding the removed key appends it at the end in
/// PHP insertion order.
#[test]
fn test_unset_assoc_then_reinsert_preserves_order() {
    let out = compile_and_run(
        r#"<?php
$m = ['a' => 1, 'b' => 2, 'c' => 3];
unset($m['a']);
$m['d'] = 4;
$m['a'] = 99;
foreach ($m as $k => $v) { echo "$k=$v "; }
echo "\n", count($m), "\n";
echo $m['c'], "\n";
"#,
    );
    assert_eq!(out, "b=2 c=3 d=4 a=99 \n4\n3\n");
}

/// Verifies `unset()` on an integer-keyed associative array removes the matching entry.
#[test]
fn test_unset_assoc_int_key() {
    let out = compile_and_run(
        r#"<?php
$m = [0 => 'x', 1 => 'y', 2 => 'z'];
unset($m[1]);
foreach ($m as $k => $v) { echo "$k=$v "; }
echo "\n", count($m), "\n";
"#,
    );
    assert_eq!(out, "0=x 2=z \n2\n");
}

/// Verifies copy-on-write: removing a key from a copy of an associative array does not mutate
/// the shared original.
#[test]
fn test_unset_assoc_copy_on_write() {
    let out = compile_and_run(
        r#"<?php
$a = ['x' => 1, 'y' => 2, 'z' => 3];
$b = $a;
unset($b['x']);
echo "a:"; foreach ($a as $k => $v) { echo " $k=$v"; }
echo "\nb:"; foreach ($b as $k => $v) { echo " $k=$v"; }
echo "\n";
"#,
    );
    assert_eq!(out, "a: x=1 y=2 z=3\nb: y=2 z=3\n");
}

/// Verifies removing entries that own heap payloads (a string and a nested array) releases them
/// without corrupting the surviving entries.
#[test]
fn test_unset_assoc_releases_heap_values() {
    let out = compile_and_run(
        r#"<?php
$m = ['s' => 'hello world', 'arr' => [1, 2, 3], 'n' => 5];
unset($m['s']);
unset($m['arr']);
foreach ($m as $k => $v) { echo "$k=$v "; }
echo "\n", count($m), "\n";
"#,
    );
    assert_eq!(out, "n=5 \n1\n");
}

/// Verifies repeatedly setting and unsetting an associative-array key in a bounded heap does not
/// leak storage (the loop would exhaust the heap if the removed values were not released).
#[test]
fn test_unset_assoc_no_leak_under_churn() {
    let out = compile_and_run(
        r#"<?php
$m = [];
for ($i = 0; $i < 5000; $i++) {
    $m['key'] = "value-" . $i;
    unset($m['key']);
}
echo count($m), "\n";
echo "done\n";
"#,
    );
    assert_eq!(out, "0\ndone\n");
}

/// Verifies unsetting a key that is absent from an associative array is a no-op.
#[test]
fn test_unset_assoc_missing_key_is_noop() {
    let out = compile_and_run(
        r#"<?php
$m = ['a' => 1, 'b' => 2];
unset($m['zzz']);
echo count($m), "\n";
foreach ($m as $k => $v) { echo "$k=$v "; }
echo "\n";
"#,
    );
    assert_eq!(out, "2\na=1 b=2 \n");
}

/// Verifies `unset($arr[$key])` on a packed indexed array removes the element without renumbering
/// the survivors: PHP keeps the original keys (a hole), so the array becomes sparse/associative.
#[test]
fn test_unset_indexed_creates_hole() {
    let out = compile_and_run(
        r#"<?php
$arr = [1, 2, 3];
unset($arr[1]);
foreach ($arr as $k => $v) { echo "$k=$v "; }
echo "\n", count($arr), "\n";
echo isset($arr[1]) ? "has1\n" : "no1\n";
echo isset($arr[2]) ? "has2\n" : "no2\n";
"#,
    );
    assert_eq!(out, "0=1 2=3 \n2\nno1\nhas2\n");
}

/// Verifies that appending after an indexed unset continues at `max_key + 1`, matching PHP.
#[test]
fn test_unset_indexed_then_append_continues_max_key() {
    let out = compile_and_run(
        r#"<?php
$arr = [1, 2, 3];
unset($arr[1]);
$arr[] = 9;
foreach ($arr as $k => $v) { echo "$k=$v "; }
echo "\n";
"#,
    );
    assert_eq!(out, "0=1 2=3 3=9 \n");
}

/// Verifies indexed-array element unset inside a function local (the array is converted to a hash
/// at the unset site).
#[test]
fn test_unset_indexed_in_function_local() {
    let out = compile_and_run(
        r#"<?php
function dump(): void {
    $arr = [10, 20, 30, 40];
    unset($arr[1]);
    foreach ($arr as $k => $v) { echo "$k=$v "; }
    echo "\n";
}
dump();
"#,
    );
    assert_eq!(out, "0=10 2=30 3=40 \n");
}

/// Verifies indexed-array element unset on a by-value array parameter.
#[test]
fn test_unset_indexed_by_value_param() {
    let out = compile_and_run(
        r#"<?php
function strip(array $a): int {
    unset($a[1]);
    return count($a);
}
echo strip([1, 2, 3]), "\n";
"#,
    );
    assert_eq!(out, "2\n");
}

/// Verifies copy-on-write for the indexed-unset conversion path: removing an element from a copy
/// does not mutate the shared original packed array.
#[test]
fn test_unset_indexed_copy_on_write() {
    let out = compile_and_run(
        r#"<?php
$a = [1, 2, 3, 4];
$b = $a;
unset($b[1]);
echo "a:"; foreach ($a as $k => $v) { echo " $k=$v"; }
echo "\nb:"; foreach ($b as $k => $v) { echo " $k=$v"; }
echo "\n";
"#,
    );
    assert_eq!(out, "a: 0=1 1=2 2=3 3=4\nb: 0=1 2=3 3=4\n");
}

/// Verifies unsetting an element of an empty array is a no-op (the array stays empty and can still
/// be appended to afterwards).
#[test]
fn test_unset_indexed_empty_array_noop() {
    let out = compile_and_run(
        r#"<?php
$arr = [];
unset($arr[0]);
echo count($arr), "\n";
$arr[] = 5;
echo $arr[0], "\n";
"#,
    );
    assert_eq!(out, "0\n5\n");
}

/// Verifies isset string offset respects bounds.
#[test]
fn test_isset_string_offset_respects_bounds() {
    let out = compile_and_run(
        r#"<?php
$s = "abc";
echo isset($s[0]) ? "y\n" : "n\n";
echo isset($s[3]) ? "y\n" : "n\n";
echo isset($s[-1]) ? "y\n" : "n\n";
echo isset($s[-4]) ? "y\n" : "n\n";
"#,
    );
    assert_eq!(out, "y\nn\ny\nn\n");
}

/// Verifies isset array offset respects bounds for non scalar elements.
#[test]
fn test_isset_array_offset_respects_bounds_for_non_scalar_elements() {
    let out = compile_and_run(
        r#"<?php
$a = ["x"];
echo isset($a[0]) ? "y\n" : "n\n";
echo isset($a[1]) ? "y\n" : "n\n";
"#,
    );
    assert_eq!(out, "y\nn\n");
}

/// Verifies isset null variable is false.
#[test]
fn test_isset_null_variable_is_false() {
    // `isset` is a bool: `isset($x)` on null is `false`, which echoes as "" (not
    // "0") in PHP; `isset($y)` on 0 is `true`, echoing "1". So the result is "1".
    let out = compile_and_run("<?php $x = null; $y = 0; echo isset($x); echo isset($y);");
    assert_eq!(out, "1");
}

/// Verifies array values.
#[test]
fn test_array_values() {
    let out = compile_and_run(
        r#"<?php $a = [10, 20, 30]; $v = array_values($a); foreach ($v as $x) { echo $x; }"#,
    );
    assert_eq!(out, "102030");
}

/// Verifies die.
#[test]
fn test_die() {
    let out = compile_and_run("<?php echo \"before\"; die(); echo \"after\";");
    assert_eq!(out, "before");
}

// --- Nested control flow ---

/// Verifies nested if.
#[test]
fn test_nested_if() {
    let out = compile_and_run(
        "<?php $x = 5; if ($x > 0) { if ($x > 3) { echo \"big\"; } else { echo \"small\"; } }",
    );
    assert_eq!(out, "big");
}

/// Verifies nested loops.
#[test]
fn test_nested_loops() {
    let out = compile_and_run(
        "<?php for ($i = 0; $i < 3; $i++) { for ($j = 0; $j < 2; $j++) { echo $i . $j . \" \"; } }",
    );
    assert_eq!(out, "00 01 10 11 20 21 ");
}

/// Verifies for continue.
#[test]
fn test_for_continue() {
    let out =
        compile_and_run("<?php for ($i = 0; $i < 5; $i++) { if ($i == 2) { continue; } echo $i; }");
    assert_eq!(out, "0134");
}

/// Verifies while with function.
#[test]
fn test_while_with_function() {
    let out = compile_and_run(
        r#"<?php
function sum_to($n) {
    $s = 0;
    $i = 1;
    while ($i <= $n) {
        $s = $s + $i;
        $i++;
    }
    return $s;
}
echo sum_to(10);
"#,
    );
    assert_eq!(out, "55");
}

/// Verifies function with if return.
#[test]
fn test_function_with_if_return() {
    let out = compile_and_run(
        r#"<?php
function abs_val($x) {
    if ($x < 0) {
        return -$x;
    }
    return $x;
}
echo abs_val(-5) . " " . abs_val(3);
"#,
    );
    assert_eq!(out, "5 3");
}

/// Verifies function calling function.
#[test]
fn test_function_calling_function() {
    let out = compile_and_run(
        r#"<?php
function square($x) { return $x * $x; }
function sum_of_squares($a, $b) { return square($a) + square($b); }
echo sum_of_squares(3, 4);
"#,
    );
    assert_eq!(out, "25");
}

/// Verifies multiple elseif.
#[test]
fn test_multiple_elseif() {
    let out = compile_and_run(
        r#"<?php
$x = 4;
if ($x == 1) { echo "one"; }
elseif ($x == 2) { echo "two"; }
elseif ($x == 3) { echo "three"; }
elseif ($x == 4) { echo "four"; }
else { echo "other"; }
"#,
    );
    assert_eq!(out, "four");
}

/// Regression: `in_array()` with a string needle must work over an indexed `array<Mixed>`. A
/// function whose container return is built from an untyped parameter is lowered to `array<Mixed>`
/// (each element a boxed Mixed cell), as is a `foreach`-value collected into a fresh array. Before
/// the fix the backend rejected `in_array(Str, array<Mixed>)` with an "unsupported" error; the
/// scan now unboxes each cell and string-compares the string-tagged ones.
#[test]
fn test_in_array_string_needle_over_mixed_array() {
    let out = compile_and_run(
        r#"<?php
function collect($x) { $r = []; $r[] = $x; return $r; }
$a = collect("hello");
$names = [];
foreach (["alpha", "beta", "gamma"] as $n) { $names[] = $n; }
echo (in_array("hello", $a) ? "y" : "n"),
     (in_array("beta", $names) ? "y" : "n"),
     (in_array("missing", $names) ? "y" : "n");
"#,
    );
    assert_eq!(out, "yyn");
}

// --- Long-form `array(...)` literal ---

/// Verifies that the long-form `array(...)` produces an indexed array equivalent to `[...]`.
#[test]
fn test_long_array_indexed() {
    let out = compile_and_run("<?php $a = array(10, 20, 30); echo count($a) . \":\" . $a[0] . \":\" . $a[2];");
    assert_eq!(out, "3:10:30");
}

/// Verifies that an empty long-form `array()` is an empty array.
#[test]
fn test_long_array_empty() {
    let out = compile_and_run("<?php $a = array(); echo count($a);");
    assert_eq!(out, "0");
}

/// Verifies that long-form `array("k" => v)` produces an associative array with the given keys.
#[test]
fn test_long_array_assoc() {
    let out = compile_and_run(
        "<?php $m = array(\"a\" => 1, \"b\" => 2); echo $m[\"a\"] + $m[\"b\"];",
    );
    assert_eq!(out, "3");
}

/// Verifies that a runtime-valued key works in a long-form `array($k => v)` literal.
#[test]
fn test_long_array_dynamic_key() {
    let out = compile_and_run("<?php $k = \"dyn\"; $kv = array($k => 42); echo $kv[\"dyn\"];");
    assert_eq!(out, "42");
}

/// Verifies that long-form arrays nest like the short form.
#[test]
fn test_long_array_nested() {
    let out = compile_and_run(
        "<?php $n = array(\"x\" => array(1, 2), \"y\" => 3); echo count($n[\"x\"]) . \":\" . $n[\"y\"];",
    );
    assert_eq!(out, "2:3");
}

/// Verifies mixed positional and keyed entries in a long-form array (positional elements keep
/// their auto-incremented integer keys around the explicit string key, as in PHP).
#[test]
fn test_long_array_mixed_positional_and_keyed() {
    let out = compile_and_run(
        "<?php $m = array(10, \"k\" => 20, 30); echo $m[0] . \":\" . $m[\"k\"] . \":\" . $m[1];",
    );
    assert_eq!(out, "10:20:30");
}

/// Verifies that spread (`...`) works inside a long-form array literal.
#[test]
fn test_long_array_spread() {
    let out = compile_and_run("<?php $s = array(...array(1, 2), 3); echo count($s);");
    assert_eq!(out, "3");
}

/// Verifies that the long-form keyword is case-insensitive (`ARRAY(...)`), matching PHP.
#[test]
fn test_long_array_case_insensitive() {
    let out = compile_and_run("<?php $a = ARRAY(1, 2); echo count($a);");
    assert_eq!(out, "2");
}

/// Verifies that the short `[...]` and long `array(...)` forms interoperate: a long-form array
/// passed to a builtin (`array_merge`) combines with a short-form array as expected.
#[test]
fn test_long_array_interops_with_short_form() {
    let out = compile_and_run(
        "<?php $a = array(1, 2); $b = [3, 4]; $c = array_merge($a, $b); echo count($c) . \":\" . $c[0] . \":\" . $c[3];",
    );
    assert_eq!(out, "4:1:4");
}

// --- References into array elements (#80, M2/M3) ---

/// Smallest end-to-end reference-into-array-element case: `$a['x'] =& $v` aliases a scalar local
/// into a Mixed associative-array element, so a later write to the source local is observed through
/// the element. The element and the source share one reference cell. Matches `php -r` output `5`.
#[test]
fn test_reference_into_assoc_element_shares_source_writes() {
    let out = compile_and_run(
        "<?php $a = ['k' => 's', 'n' => 1]; $v = 1; $a['x'] =& $v; $v = 5; echo $a['x'];",
    );
    assert_eq!(out, "5");
}

/// Verifies the aliased source local and the shared element stay in sync: after `$a['x'] =& $v`,
/// reading either `$a['x']` or `$v` reflects the latest write through the shared reference cell.
#[test]
fn test_reference_into_assoc_element_source_and_element_in_sync() {
    let out = compile_and_run(
        "<?php $a = ['k' => 's', 'n' => 1]; $v = 10; $a['x'] =& $v; $v = 7; echo $a['x'], '|', $v;",
    );
    assert_eq!(out, "7|7");
}

/// Verifies M3 base promotion: a reference may target an element of an initially **empty** array.
/// The base is promoted to a Mixed associative array on the reference assignment, so `$a['x']`
/// observes later writes to the aliased source. Matches `php -r` output `5`.
#[test]
fn test_reference_into_empty_array_promotes_base() {
    let out = compile_and_run("<?php $a = []; $v = 1; $a['x'] =& $v; $v = 5; echo $a['x'];");
    assert_eq!(out, "5");
}

/// Verifies M3 base promotion from an **indexed** array: the existing integer-keyed elements survive
/// the indexed→hash promotion while the new reference element tracks the aliased source. Matches
/// `php -r` output `7|10`.
#[test]
fn test_reference_into_indexed_array_promotes_and_preserves_elements() {
    let out = compile_and_run(
        "<?php $a = [10, 20]; $v = 1; $a['x'] =& $v; $v = 7; echo $a['x'], '|', $a[0];",
    );
    assert_eq!(out, "7|10");
}

/// Verifies M3 two-level nested reference into an **existing** inner array: `$a['k']['x'] =& $v`
/// shares the aliased source, and a later write to the source is observed when reading the nested
/// element back through the Mixed hash path. Matches `php -r` output `7`.
#[test]
fn test_reference_into_nested_existing_inner_shares_source() {
    let out = compile_and_run(
        "<?php $a = ['k' => [1, 2], 'n' => 0]; $v = 9; $a['k']['x'] =& $v; $v = 7; echo $a['k']['x'];",
    );
    assert_eq!(out, "7");
}

/// Verifies M3 two-level nested reference with an **absent** outer key: the inner array is
/// auto-vivified at the reference assignment, so `$scoped['scope']['name'] =& $v` works on an
/// initially empty base. Matches `php -r` output `8`.
#[test]
fn test_reference_into_nested_autovivified_inner_shares_source() {
    let out = compile_and_run(
        "<?php $scoped = []; $v = 5; $scoped['scope']['name'] =& $v; $v = 8; echo $scoped['scope']['name'];",
    );
    assert_eq!(out, "8");
}

/// Verifies the DeepClone accumulation pattern: two reference entries under the same outer key
/// (`$scoped['cls']['x']` and `$scoped['cls']['y']`) coexist and each tracks its own aliased source.
/// This is the shape `symfony/polyfill-deepclone` builds. Matches `php -r` output `10|20`.
#[test]
fn test_reference_into_nested_accumulates_under_same_outer_key() {
    let out = compile_and_run(
        "<?php $scoped = []; $a = 1; $b = 2; $scoped['cls']['x'] =& $a; $scoped['cls']['y'] =& $b; $a = 10; $b = 20; echo $scoped['cls']['x'], '|', $scoped['cls']['y'];",
    );
    assert_eq!(out, "10|20");
}

/// Verifies M3 reference write-through: assigning a plain value to a reference element updates the
/// shared cell in place rather than replacing the entry, so the aliased source observes the new
/// value. `$a['x'] =& $v; $a['x'] = 9;` makes `$v` read back as 9. Matches `php -r` output `9|9`.
#[test]
fn test_write_through_reference_element_updates_source() {
    let out = compile_and_run(
        "<?php $a = []; $v = 1; $a['x'] =& $v; $a['x'] = 9; echo $v, '|', $a['x'];",
    );
    assert_eq!(out, "9|9");
}

/// Verifies a Mixed-typed reference source: a value read out of a heterogeneous (Mixed-valued)
/// array is a boxed Mixed, and aliasing it into another element shares the value. Writing through
/// the alias is observed when reading the element back. Matches `php -r` output `99`.
#[test]
fn test_reference_source_mixed_write_through_alias() {
    let out = compile_and_run(
        "<?php $src = ['k' => 1, 'n' => 's']; $m = $src['k']; $b = []; $b['x'] =& $m; $m = 99; echo $b['x'];",
    );
    assert_eq!(out, "99");
}

/// Verifies a Mixed reference source is read back correctly straight after aliasing, with no
/// write-through in between: the reference cell holds the unboxed value and reconstructs it on
/// read. `$m` comes from a Mixed-valued array, so its static type is Mixed. Matches `php -r` `7`.
#[test]
fn test_reference_source_mixed_direct_element_read() {
    let out = compile_and_run(
        "<?php $src = ['k' => 7, 'n' => 's']; $m = $src['k']; $b = []; $b['x'] =& $m; echo $b['x'];",
    );
    assert_eq!(out, "7");
}

/// Verifies that writing a plain value through a reference element updates a Mixed-typed aliased
/// source: `$arr['x'] =& $m; $arr['x'] = 42;` makes the Mixed `$m` read back as 42. Exercises the
/// Mixed alias-read path (the dereferenced triple is re-boxed). Matches `php -r` output `42`.
#[test]
fn test_reference_element_write_through_updates_mixed_alias() {
    let out = compile_and_run(
        "<?php $src = ['a' => 7, 'b' => 's']; $m = $src['a']; $arr = []; $arr['x'] =& $m; $arr['x'] = 42; echo $m;",
    );
    assert_eq!(out, "42");
}

/// Verifies the reverse-direction reference assignment `$r =& $a[0]`: the variable aliases the
/// array element, so writing through the element is observed when reading the variable. This is the
/// M3 milestone shape. Matches `php -r` output `9`.
#[test]
fn test_reference_reverse_direction_aliases_element() {
    let out = compile_and_run("<?php $a = [1, 2]; $r =& $a[0]; $a[0] = 9; echo $r;");
    assert_eq!(out, "9");
}

/// Verifies the reverse-direction reference also shares in the other direction: writing through the
/// aliasing variable `$r` updates the array element it was bound to. Matches `php -r` output `7|7`.
#[test]
fn test_reference_reverse_direction_write_through_variable() {
    let out = compile_and_run("<?php $a = [10, 20]; $r =& $a[0]; $r = 7; echo $a[0], '|', $r;");
    assert_eq!(out, "7|7");
}

/// Verifies a foreach-by-reference value used as a reference source (the DeepClone pattern): each
/// `$scoped[$k] =& $value` promotes the original array entry into a shared reference cell, so the
/// source array and the target array alias the same value. Writing through the target propagates
/// back to the source array, and untouched keys stay shared. The source array is heterogeneous
/// (Mixed-valued) so reads dereference the reference entries. Matches `php -r` output `99|99|x|x`.
#[test]
fn test_reference_source_foreach_by_ref_shares_with_origin() {
    let out = compile_and_run(
        "<?php $vars = ['a' => 1, 'b' => 'x']; $scoped = []; \
         foreach ($vars as $name => &$value) { $scoped[$name] =& $value; } unset($value); \
         $scoped['a'] = 99; echo $vars['a'], '|', $scoped['a'], '|', $vars['b'], '|', $scoped['b'];",
    );
    assert_eq!(out, "99|99|x|x");
}

/// M5: `$obj->prop =& $v` aliases a stdClass dynamic property to a scalar local through the
/// object's property hash. Writing the source variable is observed when reading the property back
/// (the read dereferences the tag-11 reference entry via `__rt_stdclass_get`). Matches `php -r` `5`.
#[test]
fn test_reference_into_object_property_shares_source() {
    let out = compile_and_run(
        "<?php $o = new stdClass(); $v = 1; $o->p =& $v; $v = 5; echo $o->p;",
    );
    assert_eq!(out, "5");
}

/// M5: writing the aliased property `$obj->prop = 9` propagates back to the source variable, because
/// the property write overwrites a tag-11 reference entry and `__rt_hash_set` writes through the
/// shared reference cell (the boxed value is unboxed by `__rt_refcell_store`). Matches `php -r` `9`.
#[test]
fn test_reference_into_object_property_write_through_variable() {
    let out = compile_and_run(
        "<?php $o = new stdClass(); $v = 1; $o->p =& $v; $o->p = 9; echo $v;",
    );
    assert_eq!(out, "9");
}

/// M5: two distinct properties aliasing two distinct source variables stay independent — each
/// reference entry shares only its own cell. Writing both sources is observed through both
/// properties. Matches `php -r` output `10|20`.
#[test]
fn test_reference_into_two_object_properties_independent() {
    let out = compile_and_run(
        "<?php $o = new stdClass(); $a = 1; $b = 2; $o->x =& $a; $o->y =& $b; \
         $a = 10; $b = 20; echo $o->x, '|', $o->y;",
    );
    assert_eq!(out, "10|20");
}

/// Regression: reading a reference-aliased source variable after writing through the array element
/// on the other side must observe the fresh value. Constant propagation must not substitute the
/// source's last directly-assigned constant once it is reference-escaped. Matches `php -r` `9`.
#[test]
fn test_reference_source_read_is_fresh_after_element_write_through() {
    let out = compile_and_run(
        "<?php $a = ['x' => 0, 'n' => 's']; $v = 1; $a['x'] =& $v; $v = 5; $a['x'] = 9; echo $v;",
    );
    assert_eq!(out, "9");
}

/// Regression: a local reference (`$a =& $b`) aliases both names to one storage, so reading either
/// after writing the other must observe the fresh value — neither may be constant-propagated.
/// Matches `php -r` `9`.
#[test]
fn test_local_reference_read_is_fresh_after_cross_write() {
    let out = compile_and_run("<?php $b = 10; $a =& $b; $a = 5; $b = 9; echo $a;");
    assert_eq!(out, "9");
}

/// Regression: a `foreach (... as &$value)` binding persists after the loop and aliases the last
/// element, so reading `$value` after writing through the array must observe the fresh value rather
/// than a propagated constant. Matches `php -r` `9`.
#[test]
fn test_foreach_by_ref_value_read_is_fresh_after_cross_write() {
    let out = compile_and_run(
        "<?php $a = [1, 2, 3]; foreach ($a as &$v) {} $v = 5; $a[2] = 9; echo $v;",
    );
    assert_eq!(out, "9");
}
