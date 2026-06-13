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

/// Verifies in array not found.
#[test]
fn test_in_array_not_found() {
    let out = compile_and_run("<?php $a = [10, 20, 30]; echo in_array(99, $a);");
    assert_eq!(out, "0");
}

/// Verifies in array string found.
#[test]
fn test_in_array_string_found() {
    let out = compile_and_run(r#"<?php $a = ["a", "b", "c"]; echo in_array("b", $a);"#);
    assert_eq!(out, "1");
}

/// Verifies in array string not found.
#[test]
fn test_in_array_string_not_found() {
    let out = compile_and_run(r#"<?php $a = ["a", "b", "c"]; echo in_array("x", $a);"#);
    assert_eq!(out, "0");
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
    assert_eq!(out, "100110");
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
    let out = compile_and_run("<?php $x = null; $y = 0; echo isset($x); echo isset($y);");
    assert_eq!(out, "01");
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
