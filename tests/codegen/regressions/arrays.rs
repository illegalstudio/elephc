//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of array regressions,
//! including array push metadata, associative append keys, and bounds behavior.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

/// Verifies that the builtin `array_push` function is recognized by `function_exists`.
/// Fixture: simple `function_exists("array_push")` call.
#[test]
fn test_function_exists_builtin_array_push() {
    let out = compile_and_run(r#"<?php echo function_exists("array_push") ? "yes" : "no";"#);
    assert_eq!(out, "yes");
}

// --- Issue #12: preg_split with \s shorthand ---

/// Verifies that a negative integer array index returns null instead of corrupt data.
/// Fixture: 3-element array accessed at index `-1`.
#[test]
fn test_negative_array_index_returns_null() {
    let out = compile_and_run(
        r#"<?php
$a = [10, 20, 30];
$v = $a[-1];
if (is_null($v)) { echo "null"; } else { echo "not null"; }
"#,
    );
    assert_eq!(out, "null");
}

/// Verifies that an out-of-bounds integer array index returns null.
/// Fixture: 3-element array accessed at index `5`.
#[test]
fn test_array_out_of_bounds_returns_null() {
    let out = compile_and_run_capture(
        r#"<?php
$a = [10, 20, 30];
$v = $a[5];
if (is_null($v)) { echo "null"; } else { echo "not null"; }
"#,
    );
    assert!(out.success);
    assert_eq!(out.stdout, "null");
    assert!(out.stderr.contains("Warning: Undefined array key 5"));
}

/// Verifies missing indexed-array reads emit PHP's undefined-key warning.
/// Issue #293: the nested receiver expression must still run exactly once.
#[test]
fn test_array_out_of_bounds_warns_and_preserves_index_side_effects() {
    let out = compile_and_run_capture(
        r#"<?php
function bump(&$i) { $i++; return $i - 1; }
$arr = [["ok"], []];
$i = 0;
var_dump($arr[bump($i)][1]);
echo "i=$i\n";
"#,
    );
    assert!(out.success);
    assert_eq!(out.stdout, "NULL\ni=1\n");
    assert!(out.stderr.contains("Warning: Undefined array key 1"));
}

/// Verifies that valid integer indices still work correctly after the null-bounds check.
/// Fixture: 3-element array accessed at indices `0`, `1`, and `2`.
#[test]
fn test_array_valid_index_still_works() {
    let out = compile_and_run(
        r#"<?php
$a = [10, 20, 30];
echo $a[0] . "|" . $a[1] . "|" . $a[2];
"#,
    );
    assert_eq!(out, "10|20|30");
}

/// Verifies that float indexed-array keys are truncated to PHP integer keys on write and read.
/// Issue #302: float keys must not use stale integer registers and grow the array until heap exhaustion.
#[test]
fn test_float_array_key_assignment_and_read_truncate_to_int() {
    let out = compile_and_run(
        r#"<?php
$a = [];
$a[1.9] = 3;
$b = [10, 20];
echo $a[1] . "|" . $a[1.2] . "|" . $b[1.9];
"#,
    );
    assert_eq!(out, "3|3|20");
}

/// Verifies that multiple float keys that truncate to the same integer key update one slot.
/// Issue #302: repeated float-key writes should replace the integer slot instead of crashing.
#[test]
fn test_float_array_key_collisions_replace_integer_slot() {
    let out = compile_and_run(
        r#"<?php
$a = [];
$a[1.2] = "x";
$a[1.8] = "y";
foreach ($a as $k => $v) {
    echo $k, ":", $v, "\n";
}
"#,
    );
    assert_eq!(out, "1:y\n");
}

// -- Issue #20: assoc array missing key should return null, not garbage --

/// Verifies that accessing a missing key in an associative array returns null (not garbage).
/// Issue #20: assoc array missing key should return null, not garbage.
/// Fixture: assoc array `["a" => 1]` accessed at key `"missing"`.
#[test]
fn test_assoc_array_missing_key_returns_null() {
    let out = compile_and_run(
        r#"<?php
$m = ["a" => 1];
echo $m["missing"];
"#,
    );
    assert_eq!(out, "");
}

// -- Issue #28: array_map should handle string return values from callbacks --

/// Verifies that `array_map` correctly handles string return values from callbacks.
/// Issue #28: array_map should handle string return values from callbacks.
/// Fixture: `array_map(fn($x) => "v" . $x, [1, 2, 3])`, checks first element.
#[test]
fn test_array_map_str_callback() {
    let out = compile_and_run(
        r#"<?php
$r = array_map(fn($x) => "v" . $x, [1, 2, 3]);
echo $r[0];
"#,
    );
    assert_eq!(out, "v1");
}

/// Verifies that `array_map` with string callback returns all elements correctly.
/// Fixture: `array_map(fn($x) => "item" . $x, [1, 2, 3])`, checks all three elements.
#[test]
fn test_array_map_str_callback_all_elements() {
    let out = compile_and_run(
        r#"<?php
$r = array_map(fn($x) => "item" . $x, [1, 2, 3]);
echo $r[0] . "|" . $r[1] . "|" . $r[2];
"#,
    );
    assert_eq!(out, "item1|item2|item3");
}

// -- Issue #13: empty array literal should be accepted by type checker --

/// Verifies that an empty array literal `[]` is accepted by the type checker and can be grown.
/// Issue #13: empty array literal should be accepted by type checker.
/// Fixture: `$a = []; $a[] = 1; count($a)`.
#[test]
fn test_empty_array_literal() {
    let out = compile_and_run(
        r#"<?php
$a = [];
$a[] = 1;
echo count($a);
"#,
    );
    assert_eq!(out, "1");
}

/// Verifies that an empty array literal can be passed to `json_encode`.
/// Fixture: `json_encode([])` returns `"[]"`.
#[test]
fn test_empty_array_json_encode() {
    let out = compile_and_run(
        r#"<?php
echo json_encode([]);
"#,
    );
    assert_eq!(out, "[]");
}

// -- Issue #16: Spread operator unpacking into named parameters --

/// Verifies that chained array builtin calls (implode + array_reverse) produce correct output.
/// Fixture: `implode(",", array_reverse([3, 1, 2]))` returns `"2,1,3"`.
#[test]
fn test_implode_chained_array_builtins() {
    let out = compile_and_run(
        r#"<?php
echo implode(",", array_reverse([3, 1, 2]));
"#,
    );
    assert_eq!(out, "2,1,3");
}

/// Verifies that appending a string to an empty array via `$arr[]` works.
/// Fixture: `$a = []; $a[] = "hello"; echo $a[0]`.
#[test]
fn test_array_push_string_to_empty() {
    let out = compile_and_run(
        r#"<?php
$a = [];
$a[] = "hello";
echo $a[0];
"#,
    );
    assert_eq!(out, "hello");
}

/// Verifies that appending a concatenated expression to an array via `$arr[]` works.
/// Fixture: `$tokens = []; $word = "42"; $tokens[] = "NUM:" . $word;`.
#[test]
fn test_array_push_concat_expr() {
    let out = compile_and_run(
        r#"<?php
$tokens = [];
$word = "42";
$tokens[] = "NUM:" . $word;
echo $tokens[0];
"#,
    );
    assert_eq!(out, "NUM:42");
}

/// Verifies that pass-by-reference array mutation via index assignment works.
/// Issue #32: pass-by-reference array mutation via index assignment.
/// Fixture: `swap(&$a)` exchanges `$a[0]` and `$a[1]` in place.
#[test]
fn test_ref_array_assign() {
    // Issue #32: pass-by-reference array mutation via index assignment
    let out = compile_and_run(
        r#"<?php
function swap(&$a) {
    $t = $a[0];
    $a[0] = $a[1];
    $a[1] = $t;
}
$x = [1, 2];
swap($x);
echo $x[0];
echo $x[1];
"#,
    );
    assert_eq!(out, "21");
}

/// Verifies that pass-by-reference array mutation via `$arr[]` push works.
/// Issue #32: pass-by-reference array mutation via push.
/// Fixture: `append(&$arr, $val)` appends via `$arr[]`.
#[test]
fn test_ref_array_push() {
    // Issue #32: pass-by-reference array mutation via push
    let out = compile_and_run(
        r#"<?php
function append(&$arr, $val) {
    $arr[] = $val;
}
$x = [10, 20];
append($x, 30);
echo count($x);
echo $x[2];
"#,
    );
    assert_eq!(out, "330");
}

/// Verifies the checked-in large-arity by-ref array mutation stress example keeps
/// the late by-reference array parameter connected to the caller's storage.
#[test]
fn test_large_arity_by_ref_array_mutation_example() {
    let out = compile_and_run(include_str!(
        "../../../examples/large-by-ref-array-mutation/main.php"
    ));
    assert_eq!(out, "41|42\n");
}

/// Verifies that appending after a negative integer key uses the next PHP auto key.
/// Issue #305: appending after `$a[-2] = ...` should insert at key `-1`, not drop the value.
#[test]
fn test_append_after_negative_assoc_key_preserves_next_key() {
    let out = compile_and_run(
        r#"<?php
$a = [];
$a[-2] = 10;
$a[] = 20;
foreach ($a as $k => $v) {
    echo $k, ":", $v, "\n";
}
"#,
    );
    assert_eq!(out, "-2:10\n-1:20\n");
}

/// Verifies that appending to a string-key-only associative array starts at integer key zero.
/// Fixture: `["name" => 10]` followed by `$a[] = 20` should preserve both entries.
#[test]
fn test_append_to_string_key_assoc_array_starts_at_zero() {
    let out = compile_and_run(
        r#"<?php
$a = ["name" => 10];
$a[] = 20;
foreach ($a as $k => $v) {
    echo $k, ":", $v, "\n";
}
"#,
    );
    assert_eq!(out, "name:10\n0:20\n");
}

/// Verifies that writing to two different computed indices of a by-ref array does not corrupt values.
/// Fixture: `write_two(&$arr, $base, $val1, $val2)` writes at `$arr[$base]` and `$arr[$base+1]`
/// with computed indices, called twice with different bases.
#[test]
fn test_ref_array_multi_index_write() {
    // Writing to two different computed indices of a by-ref array must not corrupt values
    let out = compile_and_run(
        r#"<?php
function write_two(&$arr, int $base, int $val1, int $val2): void {
    $arr[$base] = $val1;
    int $idx = $base + 1;
    $arr[$idx] = $val2;
}

$data = [0, 0, 0, 0, 0, 0];
write_two($data, 0, 42, 99);
echo $data[0] . "\n";
echo $data[1] . "\n";
write_two($data, 3, 77, 88);
echo $data[3] . "\n";
echo $data[4] . "\n";
"#,
    );
    assert_eq!(out, "42\n99\n77\n88\n");
}

/// Verifies that looping over a stride-3 packed array with read+write does not corrupt data.
/// Reproduces DOOM showcase bug: loop over stride-3 packed array with read+write.
/// Fixture: `process(&$data, 4)` iterates stride-3 records, conditionally rewriting
/// `$data[$base]` and `$data[$base+1]`.
#[test]
fn test_ref_array_stride_loop_multi_write() {
    // Reproduces DOOM showcase bug: loop over stride-3 packed array with read+write
    let out = compile_and_run(
        r#"<?php
function process(&$data, int $width): void {
    int $col = 0;
    while ($col < $width) {
        int $base = $col * 3;
        int $depthVal = $data[$base];
        if ($depthVal > 100) {
            $data[$base] = 50;
            int $idx1 = $base + 1;
            $data[$idx1] = 999;
        }
        $col += 1;
    }
}

$data = [];
int $i = 0;
while ($i < 4) {
    $data[] = 2147483647;
    $data[] = 0;
    $data[] = 599;
    $i += 1;
}
process($data, 4);
echo $data[0] . "\n";
echo $data[1] . "\n";
echo $data[2] . "\n";
echo $data[3] . "\n";
echo $data[4] . "\n";
echo $data[5] . "\n";
"#,
    );
    assert_eq!(out, "50\n999\n599\n50\n999\n599\n");
}

/// Verifies no register corruption when a by-ref param lives at a stack offset > 255.
/// Regression: load_at_offset used x9 as scratch at grow_ready, clobbering the
/// array index register when the by-ref param lived at stack offset > 255.
/// Fixture: 32 integer params followed by a by-ref array `$arr`, writes at
/// computed offset `$p1 * 3`, then sums and echoes all params to force many stack slots.
#[test]
fn test_ref_array_large_offset_multi_write() {
    // Regression: load_at_offset used x9 as scratch at grow_ready, clobbering the
    // array index register when the by-ref param lived at stack offset > 255.
    let out = compile_and_run(
        r#"<?php
function big(
    int $p1, int $p2, int $p3, int $p4, int $p5,
    int $p6, int $p7, int $p8, int $p9, int $p10,
    int $p11, int $p12, int $p13, int $p14, int $p15,
    int $p16, int $p17, int $p18, int $p19, int $p20,
    int $p21, int $p22, int $p23, int $p24, int $p25,
    int $p26, int $p27, int $p28, int $p29, int $p30,
    int $p31, int $p32,
    &$arr
): void {
    int $base = $p1 * 3;
    $arr[$base] = 50;
    int $idx = $base + 1;
    $arr[$idx] = 999;
    echo $p2 + $p3 + $p4 + $p5 + $p6 + $p7 + $p8 + $p9 + $p10;
    echo $p11 + $p12 + $p13 + $p14 + $p15 + $p16 + $p17 + $p18 + $p19 + $p20;
    echo $p21 + $p22 + $p23 + $p24 + $p25 + $p26 + $p27 + $p28 + $p29 + $p30;
    echo $p31 + $p32;
}
$data = [0, 0, 0, 0, 0, 0];
big(0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,$data);
echo "\n" . $data[0] . "\n" . $data[1] . "\n";
"#,
    );
    assert_eq!(out, "0000\n50\n999\n");
}

/// Verifies that `array_column` on arrays of assoc arrays with string values works with `implode`.
/// Issue #33: array_column on arrays of assoc arrays with string values + implode.
/// Fixture: `$s = [["n" => "Alice"], ["n" => "Bob"]]; array_column($s, "n")`.
#[test]
fn test_array_column_string_implode() {
    // Issue #33: array_column on arrays of assoc arrays with string values + implode
    let out = compile_and_run(
        r#"<?php
$s = [["n" => "Alice"], ["n" => "Bob"]];
$names = array_column($s, "n");
echo implode(",", $names);
"#,
    );
    assert_eq!(out, "Alice,Bob");
}

/// Verifies that hash keys survive concat_buf reset (persisted to heap).
/// Tests that computed string keys in associative arrays remain valid after
/// concat_buf is reused during a loop.
/// Fixture: `$h = ["init" => 0];` then loop appending `$h["k" . $i] = $i` for 10 iterations.
#[test]
fn test_hash_table_computed_keys_loop() {
    // Tests that hash keys survive concat_buf reset (persisted to heap)
    let out = compile_and_run(
        r#"<?php
$h = ["init" => 0];
for ($i = 0; $i < 10; $i++) {
    $h["k" . $i] = $i;
}
echo $h["k9"];
"#,
    );
    assert_eq!(out, "9");
}

/// Verifies that an integer array grows beyond initial capacity via reallocation.
/// Fixture: start with 3 elements, append 97 more via `$arr[]`, verify count and endpoints.
#[test]
fn test_array_dynamic_growth_int() {
    // Array grows beyond initial capacity via reallocation
    let out = compile_and_run(
        r#"<?php
$arr = [1, 2, 3];
for ($i = 4; $i <= 100; $i++) {
    $arr[] = $i;
}
echo count($arr) . "|" . $arr[0] . "|" . $arr[99];
"#,
    );
    assert_eq!(out, "100|1|100");
}

/// Verifies that a string array grows beyond initial capacity via reallocation.
/// Fixture: start with 1 string element, append 50 more via `$arr[]`, verify count and endpoints.
#[test]
fn test_array_dynamic_growth_str() {
    // String array grows beyond initial capacity
    let out = compile_and_run(
        r#"<?php
$arr = ["first"];
for ($i = 0; $i < 50; $i++) {
    $arr[] = "item" . $i;
}
echo count($arr) . "|" . $arr[0] . "|" . $arr[50];
"#,
    );
    assert_eq!(out, "51|first|item49");
}

/// Verifies that direct growth via `$arr[3] = 40` on an indexed array preserves existing int slots.
/// Fixture: `$arr = [10, 20, 30]; $arr[3] = 40;` check all 4 elements and count.
#[test]
fn test_indexed_array_direct_growth_preserves_int_slots() {
    let out = compile_and_run(
        r#"<?php
$arr = [10, 20, 30];
$arr[3] = 40;
echo count($arr) . "|" . $arr[0] . "|" . $arr[1] . "|" . $arr[2] . "|" . $arr[3];
"#,
    );
    assert_eq!(out, "4|10|20|30|40");
}

/// Verifies that direct growth via `$arr[2] = "c"` on an indexed array preserves existing string slots.
/// Fixture: `$arr = ["a", "b"]; $arr[2] = "c";` check all 3 elements and count.
#[test]
fn test_indexed_array_direct_growth_preserves_string_slots() {
    let out = compile_and_run(
        r#"<?php
$arr = ["a", "b"];
$arr[2] = "c";
echo count($arr) . "|" . $arr[0] . "|" . $arr[1] . "|" . $arr[2];
"#,
    );
    assert_eq!(out, "3|a|b|c");
}

/// Verifies that `array_push()` triggers array growth correctly.
/// Fixture: `$arr = [10];` loop 20 times with `array_push($arr, $i * 10)`, verify count and last element.
#[test]
fn test_array_push_function_growth() {
    // array_push() triggers growth
    let out = compile_and_run(
        r#"<?php
$arr = [10];
for ($i = 0; $i < 20; $i++) {
    array_push($arr, $i * 10);
}
echo count($arr) . "|" . $arr[20];
"#,
    );
    assert_eq!(out, "21|190");
}

/// Verifies that arrays returned from a function can be grown and reassigned multiple times.
/// Fixture: `grow($arr)` pushes 32 elements and returns the array; call it 20 times in a loop,
/// then verify final count > 100.
#[test]
fn test_array_reassign_after_function_growth() {
    let out = compile_and_run(
        r#"<?php
function grow($arr) {
    for ($i = 0; $i < 32; $i++) {
        array_push($arr, $i);
    }
    return $arr;
}

$arr = [100];
for ($j = 0; $j < 20; $j++) {
    $arr = grow($arr);
}
echo count($arr) > 100 ? "ok" : "bad";
"#,
    );
    assert_eq!(out, "ok");
}

/// Verifies that `array_push()` can push float values.
/// Fixture: `$arr = [1.1]; array_push($arr, 2.2);` check count and second element.
#[test]
fn test_array_push_float() {
    let out = compile_and_run(
        r#"<?php
$arr = [1.1];
array_push($arr, 2.2);
echo count($arr) . "|" . $arr[1];
"#,
    );
    assert_eq!(out, "2|2.2");
}

/// Verifies that `array_push()` can push boolean values.
/// Fixture: `$arr = [true]; array_push($arr, false);` check count is 2.
#[test]
fn test_array_push_bool() {
    let out = compile_and_run(
        r#"<?php
$arr = [true];
array_push($arr, false);
echo count($arr);
"#,
    );
    assert_eq!(out, "2");
}

/// Verifies that `array_push()` can push object values and read back a property.
/// Fixture: `$items = [new Item("a")]; array_push($items, new Item("b"));`.
#[test]
fn test_array_push_object() {
    let out = compile_and_run(
        r#"<?php
class Item { public $name;
    public function __construct($n) { $this->name = $n; }
}
$items = [new Item("a")];
array_push($items, new Item("b"));
echo count($items) . "|" . $items[1]->name;
"#,
    );
    assert_eq!(out, "2|b");
}

/// Verifies that appending float values via `$arr[] = float` syntax works.
/// Fixture: `$arr = [1.0]; $arr[] = 2.5; $arr[] = 3.7;` check count and last element.
#[test]
fn test_array_push_syntax_float() {
    // $arr[] = float syntax
    let out = compile_and_run(
        r#"<?php
$arr = [1.0];
$arr[] = 2.5;
$arr[] = 3.7;
echo count($arr) . "|" . $arr[2];
"#,
    );
    assert_eq!(out, "3|3.7");
}
