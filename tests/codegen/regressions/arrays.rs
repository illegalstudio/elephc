//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of regressions arrays, including function exists builtin array push, negative array index returns null, and out of bounds returns null.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

#[test]
fn test_function_exists_builtin_array_push() {
    let out = compile_and_run(r#"<?php echo function_exists("array_push") ? "yes" : "no";"#);
    assert_eq!(out, "yes");
}

// --- Issue #12: preg_split with \s shorthand ---

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

#[test]
fn test_array_out_of_bounds_returns_null() {
    let out = compile_and_run(
        r#"<?php
$a = [10, 20, 30];
$v = $a[5];
if (is_null($v)) { echo "null"; } else { echo "not null"; }
"#,
    );
    assert_eq!(out, "null");
}

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

// -- Issue #20: assoc array missing key should return null, not garbage --

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

#[test]
fn test_implode_chained_array_builtins() {
    let out = compile_and_run(
        r#"<?php
echo implode(",", array_reverse([3, 1, 2]));
"#,
    );
    assert_eq!(out, "2,1,3");
}

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
