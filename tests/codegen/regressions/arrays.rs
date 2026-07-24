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

/// Verifies `array_fill` with a non-zero start index produces keys `start..start+count-1`
/// (a keyed array) instead of ignoring the start, and that a string fill value works (the
/// scalar indexed path could not store a pointer+length). Covers int, string, float, and a
/// negative start. Before the fix non-zero start gave keys 0,1,.. and string values crashed.
#[test]
fn test_array_fill_nonzero_start_and_string_value() {
    let out = compile_and_run(
        r#"<?php
foreach (array_fill(5, 3, "x") as $k => $v) { echo "$k=$v,"; }
echo "|";
foreach (array_fill(0, 2, "s") as $k => $v) { echo "$k=$v,"; }
echo "|";
foreach (array_fill(-2, 2, 7) as $k => $v) { echo "$k=$v,"; }
echo "|";
foreach (array_fill(3, 2, 1.5) as $k => $v) { echo "$k=$v,"; }
"#,
    );
    assert_eq!(out, "5=x,6=x,7=x,|0=s,1=s,|-2=7,-1=7,|3=1.5,4=1.5,");
}

/// Verifies `array_fill(0, n, scalar)` still builds a plain 0-based indexed array (the fast,
/// unchanged path) and that its values and count are correct.
#[test]
fn test_array_fill_zero_start_scalar_stays_indexed() {
    let out = compile_and_run(
        r#"<?php $a = array_fill(0, 3, 9); echo count($a), ":", $a[0], $a[1], $a[2];"#,
    );
    assert_eq!(out, "3:999");
}

/// Verifies an un-annotated method returning an array can be stored in a local and indexed.
/// Regression: the initial type-checking pass mis-typed the method's return before method
/// signatures stabilized, and the stale "Cannot index non-array" diagnostic survived because the
/// erroring statement (`echo $r[0]`) structurally contained only an array access, not the method
/// call. The final fixpoint pass types `$r` correctly, so the stale error must be suppressed.
#[test]
fn test_untyped_method_return_array_indexed_via_local() {
    let out = compile_and_run(
        r#"<?php
class C {
    private $row;
    public function set() { $this->row = [10, 20, 30]; }
    public function get() { return $this->row; }
}
$c = new C();
$c->set();
$r = $c->get();
echo $r[0] . "," . $r[1] . "," . $r[2];
"#,
    );
    assert_eq!(out, "10,20,30");
}

/// Verifies an un-annotated method returning an assoc array survives a local round-trip and keying.
/// Regression companion to the indexed case: the result of a `mixed`-returning method stored in a
/// local and keyed by string must not raise the stale "Cannot index non-array" diagnostic.
#[test]
fn test_untyped_method_return_assoc_indexed_via_local() {
    let out = compile_and_run(
        r#"<?php
class C {
    private $row;
    public function fetch(): mixed { $a = []; $a["x"] = 10; $a["y"] = 20; return $a; }
    public function load(): void { $this->row = $this->fetch(); }
    public function current(): mixed { return $this->row; }
}
$c = new C();
$c->load();
$r = $c->current();
echo $r["x"] . "," . $r["y"];
"#,
    );
    assert_eq!(out, "10,20");
}

/// Verifies an `[]`-initialized property whose whole value is reassigned widens its element type.
/// Regression: `private $row = [];` types the property as `Array(Never)`, and a later
/// `$this->row = [10, 20, 30]` left the element type pinned at `Never`, so reading the returned
/// array produced empty output. The first concrete array assignment must fix the element type.
#[test]
fn test_empty_array_init_property_reassigned_then_indexed() {
    let out = compile_and_run(
        r#"<?php
class C {
    private $row = [];
    public function set() { $this->row = [10, 20, 30]; }
    public function get() { return $this->row; }
}
$c = new C();
$c->set();
$r = $c->get();
echo $r[0] . "," . $r[1] . "," . $r[2];
"#,
    );
    assert_eq!(out, "10,20,30");
}

/// Verifies an `[]`-initialized property reassigned then indexed inside the same method.
/// Regression: the stale `Array(Never)` element type made the method's return type collapse to
/// `Never`, raising "A never-returning function must not implicitly return".
#[test]
fn test_empty_array_init_property_reassigned_indexed_in_method() {
    let out = compile_and_run(
        r#"<?php
class C {
    private $row = [];
    public function go() { $this->row = [10, 20, 30]; return $this->row[0]; }
}
$c = new C();
echo $c->go();
"#,
    );
    assert_eq!(out, "10");
}

/// Verifies an `[]`-initialized property that receives only string-keyed writes survives being
/// returned whole from a method and re-indexed through a local.
/// Regression: the `[]` default was emitted as indexed-list storage even though the property's
/// refined type is associative, so after the array crossed the method-return boundary the hash
/// lookup missed every key and decoded to the null sentinel (9223372036854775806). The default
/// must be stored as an empty hash to match the property's associative storage.
#[test]
fn test_empty_array_init_property_string_keyed_then_returned() {
    let out = compile_and_run(
        r#"<?php
class C {
    private $row = [];
    public function set() { $this->row["a"] = 10; $this->row["b"] = 20; }
    public function get() { return $this->row; }
}
$c = new C();
$c->set();
$r = $c->get();
echo $r["a"] . "," . $r["b"];
"#,
    );
    assert_eq!(out, "10,20");
}

/// Verifies a positional-literal default (`[1, 2, 3]`) on a property later given string keys is
/// stored associatively, so the whole array survives a cross-method return and string re-indexing.
/// Regression companion: the positional default must also be rewritten to hash storage when the
/// property's refined type is associative, not left as an indexed-list array.
#[test]
fn test_positional_array_init_property_string_keyed_then_returned() {
    let out = compile_and_run(
        r#"<?php
class C {
    private $row = [1, 2, 3];
    public function set() { $this->row["a"] = 10; }
    public function get() { return $this->row; }
}
$c = new C();
$c->set();
$r = $c->get();
echo $r["a"];
"#,
    );
    assert_eq!(out, "10");
}

/// Regression: `array_fill()` with a negative count must terminate and yield an empty array on
/// every target. The ARM64 string-value path used an unsigned `cbz` loop guard, so a negative
/// count never reached zero and looped until heap exhaustion; it now matches the x86_64 signed
/// guard.
#[test]
fn test_array_fill_negative_count_string_value() {
    let out = compile_and_run(
        r#"<?php
$r = @array_fill(0, -1, "x");
echo is_array($r) ? "arr" : "no", ":", count($r);
$ok = array_fill(0, 3, "ab");
echo "|", count($ok), ":", $ok[0], $ok[2];
"#,
    );
    assert_eq!(out, "arr:0|3:abab");
}

/// Regression (issue #407): reading a typed `array` property by a *variable* string key must
/// type-check and run. The declared `array` hint stored the property as an int-keyed `Array`, so
/// `$this->data[$key]` was rejected with "Array index must be integer"; an associative literal
/// default must refine the property to associative (hash) storage so string keys are accepted.
#[test]
fn test_typed_array_property_assoc_default_variable_key() {
    let out = compile_and_run(
        r#"<?php
class Bag {
    public array $data = ['a' => 1];
    public function get(string $key): int {
        return $this->data[$key] ?? 0;
    }
}
$b = new Bag();
echo $b->get('a');
"#,
    );
    assert_eq!(out, "1");
}

/// Regression (issue #407 companion): the same typed `array` property must also accept a *literal*
/// string key both from inside a method and through direct external property access. The issue
/// claimed literal keys already worked, but on 0.25.x they failed identically until the associative
/// literal default refined the property type.
#[test]
fn test_typed_array_property_assoc_default_literal_key() {
    let out = compile_and_run(
        r#"<?php
class Bag {
    public array $data = ['a' => 1, 'b' => 2];
    public function get(): int { return $this->data['a']; }
}
$b = new Bag();
echo $b->get(), $b->data['b'];
"#,
    );
    assert_eq!(out, "12");
}

/// Regression: an *untyped* property initialized with an associative literal default
/// (`public $data = ['a' => 1]`) must lower its default through the EIR backend. Previously the
/// associative literal (`ExprKind::ArrayLiteralAssoc`) was not handled by the property-default
/// emitter, failing with "unsupported EIR backend feature: object_new for default value".
#[test]
fn test_untyped_property_assoc_literal_default() {
    let out = compile_and_run(
        r#"<?php
class Bag {
    public $data = ['a' => 1, 'b' => 2];
    public function get(string $key): int { return $this->data[$key] ?? 0; }
}
$b = new Bag();
echo $b->get('a'), $b->get('b'), $b->get('missing');
"#,
    );
    assert_eq!(out, "120");
}

/// Regression: an associative literal property default with string keys and string values must
/// round-trip its keys and values, exercising the string-keyed hash-insert path of the EIR
/// property-default emitter (key normalization plus persistent string values).
#[test]
fn test_typed_array_property_assoc_default_string_values() {
    let out = compile_and_run(
        r#"<?php
class Req {
    public array $headers = ['Host' => 'example.com', 'Accept' => 'text/html'];
    public function header(string $name): string { return $this->headers[$name] ?? ''; }
}
$r = new Req();
echo $r->header('Host'), "|", $r->header('Accept');
"#,
    );
    assert_eq!(out, "example.com|text/html");
}

/// Regression: a child may redeclare an inherited `array` property whose associative literal
/// default has a different element/value shape than the parent's. Both denote the same PHP `array`
/// hint, so refining each to its own associative storage type must not trip property-type
/// invariance (which would spuriously report "must be AssocArray<int>, not AssocArray<string>").
#[test]
fn test_redeclared_array_property_assoc_default_different_value_shape() {
    let out = compile_and_run(
        r#"<?php
class Base { public array $data = ['a' => 1]; }
class Child extends Base { public array $data = ['a' => 'x', 'b' => 'y']; }
$c = new Child();
echo $c->data['a'], $c->data['b'];
"#,
    );
    assert_eq!(out, "xy");
}

/// Regression: a static property with an associative literal default must lower through the same
/// EIR static-property initialization path, so string-key reads work on static array properties.
#[test]
fn test_static_array_property_assoc_literal_default() {
    let out = compile_and_run(
        r#"<?php
class Config {
    public static array $map = ['debug' => 1, 'verbose' => 0];
}
echo Config::$map['debug'], Config::$map['verbose'];
"#,
    );
    assert_eq!(out, "10");
}

/// Regression for issue #413: associative property defaults with heterogeneous value types must
/// infer a Mixed-valued slot for typed and untyped properties instead of rejecting prop_set.
#[test]
fn test_array_property_assoc_literal_default_heterogeneous_values() {
    let out = compile_and_run(
        r#"<?php
class TypedBag {
    public array $data = ['n' => 1, 's' => 'hi'];
    public function line(): string { return $this->data['n'] . "|" . $this->data['s']; }
}
class UntypedBag {
    public $data = ['n' => 1, 's' => 'hi'];
    public function line(): string { return $this->data['n'] . "|" . $this->data['s']; }
}
class ThreeWayBag {
    public array $data = ['a' => 1, 'b' => 'x', 'c' => 1.5];
    public function line(): string { return $this->data['a'] . "|" . $this->data['b'] . "|" . $this->data['c']; }
}
$typed = new TypedBag();
$untyped = new UntypedBag();
$three = new ThreeWayBag();
echo $typed->line(), "\n", $untyped->line(), "\n", $three->line();
"#,
    );
    assert_eq!(out, "1|hi\n1|hi\n1|x|1.5");
}

/// Regression for issue #360: indexed array elements are valid PHP l-values for
/// by-reference parameters, and mutations must write back into the array slot.
#[test]
fn test_array_element_can_be_passed_by_reference() {
    let out = compile_and_run(
        r#"<?php
function bump(&$x) { $x++; }
$a = [5];
bump($a[0]);
echo $a[0];
"#,
    );
    assert_eq!(out, "6");
}

/// Regression for issue #360: taking an array-element reference must split
/// copy-on-write storage before the callee mutates the element.
#[test]
fn test_array_element_by_reference_splits_copy_on_write_storage() {
    let out = compile_and_run(
        r#"<?php
function bump(&$x) { $x++; }
$a = [5];
$b = $a;
bump($b[0]);
echo $a[0], ":", $b[0];
"#,
    );
    assert_eq!(out, "5:6");
}

// --- Issue #526: chained subscript read with a miss on the FIRST index ---

/// Regression for issue #526: a chained subscript read whose FIRST index misses
/// must warn and propagate null instead of dereferencing the scalar null
/// sentinel as an array pointer (historical SIGSEGV in the outer length load).
#[test]
fn test_chained_read_first_index_miss_warns_and_yields_null() {
    let out = compile_and_run_capture(
        r#"<?php
$a = [];
for ($i = 0; $i < 3; $i++) { $a[] = ['kw', 'word' . $i]; }
$x = (string) $a[7][1];
echo 'x=' . $x . "\n";
echo 'done' . "\n";
"#,
    );
    assert!(out.success, "program crashed: {}", out.stderr);
    assert_eq!(out.stdout, "x=\ndone\n");
    assert!(out.stderr.contains("Warning: Undefined array key 7"));
}

/// Guard for issue #526: the already-working second-index-miss path keeps its
/// behavior — one warning, empty result, exit 0.
#[test]
fn test_chained_read_second_index_miss_still_warns_and_yields_null() {
    let out = compile_and_run_capture(
        r#"<?php
$a = [];
for ($i = 0; $i < 3; $i++) { $a[] = ['kw', 'word' . $i]; }
$x = (string) $a[1][7];
echo 'x=' . $x . "\n";
echo 'done' . "\n";
"#,
    );
    assert!(out.success, "program crashed: {}", out.stderr);
    assert_eq!(out.stdout, "x=\ndone\n");
    assert!(out.stderr.contains("Warning: Undefined array key 7"));
}

/// Regression for issue #526: the crashing chained miss must leave the heap
/// clean — the sentinel-null container must not enter refcount traffic.
#[test]
fn test_chained_read_first_index_miss_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$a = [];
for ($i = 0; $i < 3; $i++) { $a[] = ['kw', 'word' . $i]; }
$x = (string) $a[7][1];
echo 'x=' . $x . "\n";
"#,
    );
    assert!(out.success, "program crashed: {}", out.stderr);
    assert_eq!(out.stdout, "x=\n");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap, got: {}",
        out.stderr
    );
}

/// Regression for issue #526: `isset()` over a chained subscript whose first
/// index misses is false, silent, and must not crash.
#[test]
fn test_chained_isset_first_index_miss_is_false_and_silent() {
    let out = compile_and_run_capture(
        r#"<?php
$a = [['kw', 'w0']];
echo isset($a[7][1]) ? 'yes' : 'no';
"#,
    );
    assert!(out.success, "program crashed: {}", out.stderr);
    assert_eq!(out.stdout, "no");
    assert_eq!(out.stderr, "");
}

/// Regression for issue #526: `??` over a chained subscript whose first index
/// misses stays silent across the whole chain and must not crash.
#[test]
fn test_chained_coalesce_first_index_miss_is_silent() {
    let out = compile_and_run_capture(
        r#"<?php
$a = [['kw', 'w0']];
echo 'A' . ($a[7][1] ?? '') . 'B';
"#,
    );
    assert!(out.success, "program crashed: {}", out.stderr);
    assert_eq!(out.stdout, "AB");
    assert_eq!(out.stderr, "");
}

/// Regression for issue #526: a string-keyed chained miss on an assoc-of-assoc
/// yields null instead of dereferencing the sentinel as a hash table.
#[test]
fn test_chained_string_key_first_miss_yields_null() {
    let out = compile_and_run_capture(
        r#"<?php
$m = ['a' => ['x' => 1, 'y' => 2], 'b' => ['x' => 3]];
$v = $m['nope']['x'];
echo is_null($v) ? 'null' : 'notnull';
"#,
    );
    assert!(out.success, "program crashed: {}", out.stderr);
    assert_eq!(out.stdout, "null");
}

/// Regression for issue #526: a three-level chain with a miss at each level
/// yields null at every level without crashing.
#[test]
fn test_three_level_chain_miss_at_each_level_yields_null() {
    let out = compile_and_run_capture(
        r#"<?php
$a = [[[5]]];
echo is_null($a[9][0][0]) ? 'n' : 'v';
echo is_null($a[0][9][0]) ? 'n' : 'v';
echo is_null($a[0][0][9]) ? 'n' : 'v';
"#,
    );
    assert!(out.success, "program crashed: {}", out.stderr);
    assert_eq!(out.stdout, "nnn");
    assert!(out.stderr.contains("Warning: Undefined array key 9"));
}

/// Regression for issue #526: foreach over a missed element with a `?? []`
/// default iterates the default silently instead of crashing.
#[test]
fn test_foreach_over_first_index_miss_coalesce_default() {
    let out = compile_and_run_capture(
        r#"<?php
$a = [['kw', 'w0']];
foreach ($a[7] ?? [] as $v) { echo 'v=' . $v; }
echo 'done';
"#,
    );
    assert!(out.success, "program crashed: {}", out.stderr);
    assert_eq!(out.stdout, "done");
    assert_eq!(out.stderr, "");
}

/// Regression for issue #526: foreach directly over a missed element warns for
/// the miss and skips the loop body instead of crashing on the sentinel.
#[test]
fn test_foreach_over_first_index_miss_skips_loop() {
    let out = compile_and_run_capture(
        r#"<?php
$a = [['kw', 'w0']];
foreach ($a[7] as $v) { echo 'v=' . $v; }
echo 'done';
"#,
    );
    assert!(out.success, "program crashed: {}", out.stderr);
    assert_eq!(out.stdout, "done");
    assert!(out.stderr.contains("Warning: Undefined array key 7"));
}

/// Guard for issue #526: heterogeneous (Mixed-element) arrays already routed
/// misses through boxed Mixed nulls; the fixed path keeps that behavior.
#[test]
fn test_chained_read_first_index_miss_mixed_elements() {
    let out = compile_and_run_capture(
        r#"<?php
$h = [[1, 'a'], 'str', 42];
$x = $h[7][1];
echo is_null($x) ? 'null' : 'notnull';
"#,
    );
    assert!(out.success, "program crashed: {}", out.stderr);
    assert_eq!(out.stdout, "null");
    assert!(out.stderr.contains("Warning: Undefined array key 7"));
}

/// Regression for issue #526: header-reading predicates handle a null-container
/// miss without dereferencing it; direct reads warn while `empty()` stays silent.
#[test]
fn test_array_miss_truthiness_bool_cast_and_empty_are_null_safe() {
    let out = compile_and_run_capture(
        r#"<?php
$a = [[1]];
echo $a[7] ? "bad" : "false";
echo ":" . ((bool) $a[7] ? "bad" : "false");
echo ":" . (empty($a[7]) ? "empty" : "bad");
"#,
    );
    assert!(out.success, "program crashed: {}", out.stderr);
    assert_eq!(out.stdout, "false:false:empty");
    assert_eq!(out.stderr.matches("Warning: Undefined array key 7").count(), 2);
}

/// Regression for issue #526: `count()` and array spread turn a missed nested
/// array into catchable PHP errors instead of reading its null sentinel header.
#[test]
fn test_array_miss_count_and_spread_throw_catchable_errors() {
    let out = compile_and_run_capture(
        r#"<?php
$a = [[1]];
try { count($a[7]); } catch (TypeError $e) { echo $e->getMessage() . "\n"; }
try { $copy = [...$a[7]]; } catch (Error $e) { echo $e->getMessage() . "\n"; }
"#,
    );
    assert!(out.success, "program crashed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "count(): Argument #1 ($value) must be of type Countable|array, null given\n\
Only arrays and Traversables can be unpacked, null given\n"
    );
    assert_eq!(out.stderr.matches("Warning: Undefined array key 7").count(), 2);
}

/// Regression for issue #526: property and method consumers recognize the raw
/// object null sentinel produced by a missed object-array read.
#[test]
fn test_array_miss_object_property_warns_and_method_throws() {
    let out = compile_and_run_capture(
        r#"<?php
class MissBox {
    public int $value = 1;
    public function take(int $value): int { return $value; }
}
class MagicMissBox {
    public function __get(string $name): mixed { echo "bad"; return $name; }
}
function should_not_run(): int { echo "bad"; return 9; }
$objects = [new MissBox()];
var_dump($objects[7]->value);
$std = new stdClass();
$std->value = 1;
$stdObjects = [$std];
var_dump($stdObjects[7]->value);
$magicObjects = [new MagicMissBox()];
var_dump($magicObjects[7]->value);
$property = "value";
var_dump($objects[7]->{$property});
try { $objects[7]->take(should_not_run()); }
catch (Error $e) { echo $e->getMessage(); }
$method = "take";
try { $objects[7]->{$method}(should_not_run()); }
catch (Error $e) { echo ":" . $e->getMessage(); }
$miss = $objects[7];
try { $miss->{$method}(should_not_run()); }
catch (Error $e) { echo ":" . $e->getMessage(); }
echo ":" . $objects[0]->{$method}(3);
"#,
    );
    assert!(out.success, "program crashed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "NULL\nNULL\nNULL\nNULL\nCall to a member function take() on null:\
Call to a member function take() on null:\
Call to a member function take() on null:3"
    );
    assert_eq!(out.stderr.matches("Warning: Undefined array key 7").count(), 7);
    assert_eq!(
        out.stderr
            .matches("Warning: Attempt to read property \"value\" on null")
            .count(),
        4
    );
}

/// Regression for issue #526: direct associative misses emit both PHP warnings,
/// while the same chained lookup under null coalescing remains silent.
#[test]
fn test_array_miss_assoc_chained_warns_directly_and_is_silent_under_coalesce() {
    let out = compile_and_run_capture(
        r#"<?php
$map = ["present" => ["leaf" => 1]];
var_dump($map["missing"]["leaf"]);
echo $map["missing"]["leaf"] ?? 42;
"#,
    );
    assert!(out.success, "program crashed: {}", out.stderr);
    assert_eq!(out.stdout, "NULL\n42");
    assert_eq!(
        out.stderr
            .matches("Warning: Undefined array key \"missing\"")
            .count(),
        1
    );
    assert_eq!(
        out.stderr
            .matches("Warning: Trying to access array offset on null")
            .count(),
        1
    );
}

/// Regression for issue #554: a missing string-valued chained read retains its
/// null marker through ownership stabilization, while real empty strings do not.
#[test]
fn test_array_miss_string_coalesces_but_real_empty_strings_do_not() {
    let out = compile_and_run_capture(
        r#"<?php
$nested = [["present"]];
$empty = [""];
echo "[" . ($nested[7][0] ?? "fallback") . "]";
echo "[" . ($empty[0] ?? "bad") . "]";
echo "[" . (str_repeat("x", 0) ?? "bad") . "]";
"#,
    );
    assert!(out.success, "program crashed: {}", out.stderr);
    assert_eq!(out.stdout, "[fallback][][]");
    assert_eq!(out.stderr, "");
}

// --- Issue #556: by-reference foreach over a missing array element ---

/// Regression for issue #556: a by-reference foreach directly over a missed
/// element warns for the miss and skips the loop body instead of handing the
/// null-container sentinel to the copy-on-write helper.
#[test]
fn test_byref_foreach_over_first_index_miss_skips_loop() {
    let out = compile_and_run_capture(
        r#"<?php
$a = [['x', 'y']];
foreach ($a[7] as &$v) { $v = 'changed'; }
echo 'done';
"#,
    );
    assert!(out.success, "program crashed: {}", out.stderr);
    assert_eq!(out.stdout, "done");
    assert!(out.stderr.contains("Warning: Undefined array key 7"));
}

/// Guard for issue #556: the `?? []` by-reference form iterates the empty
/// default silently — the coalesce materializes a real empty array, so the
/// sentinel never reaches the iterator; this locks that behavior in place.
#[test]
fn test_byref_foreach_over_first_index_miss_coalesce_default() {
    let out = compile_and_run_capture(
        r#"<?php
$a = [['x', 'y']];
foreach ($a[7] ?? [] as &$v) { $v = 'changed'; }
echo 'done';
"#,
    );
    assert!(out.success, "program crashed: {}", out.stderr);
    assert_eq!(out.stdout, "done");
    assert_eq!(out.stderr, "");
}

/// Regression for issue #556: a string-keyed miss feeding a by-reference foreach
/// takes the same null-source path as the indexed form.
#[test]
fn test_byref_foreach_over_assoc_key_miss_skips_loop() {
    let out = compile_and_run_capture(
        r#"<?php
$h = ['k' => ['q' => 1]];
foreach ($h['nope'] as &$v) { $v = 2; }
echo 'done';
"#,
    );
    assert!(out.success, "program crashed: {}", out.stderr);
    assert_eq!(out.stdout, "done");
    assert!(out.stderr.contains("Warning: Undefined array key \"nope\""));
}

/// Control for issue #556: the null-source guards must not disturb ordinary
/// by-reference mutation and aliasing over a real array.
#[test]
fn test_byref_foreach_over_present_source_still_mutates() {
    let out = compile_and_run_capture(
        r#"<?php
$ok = ['a', 'b'];
foreach ($ok as &$v) { $v = strtoupper($v); }
unset($v);
echo implode(',', $ok);
"#,
    );
    assert!(out.success, "program crashed: {}", out.stderr);
    assert_eq!(out.stdout, "A,B");
    assert_eq!(out.stderr, "");
}

/// Regression for issue #556: the skipped by-reference loop must leave the heap
/// clean — the sentinel source must never enter refcount or copy-on-write traffic.
#[test]
fn test_byref_foreach_over_first_index_miss_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$a = [['x', 'y']];
foreach ($a[7] as &$v) { $v = 'changed'; }
echo "done\n";
"#,
    );
    assert!(out.success, "program crashed: {}", out.stderr);
    assert_eq!(out.stdout, "done\n");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap, got: {}",
        out.stderr
    );
}

/// Regression for issue #556: the null-container sentinel must be recognized before any
/// array-header load on the by-reference foreach path, on every supported target.
/// The guard placement follows the #533 convention: the sentinel check lives inside the
/// copy-on-write runtime helpers (like `__rt_hash_iter_next`), `IterStart` folds a
/// sentinel source to the canonical zero pointer in the iterator's private slot, and the
/// per-iteration `IterNext` live-length read needs only the cheap zero check.
/// Run under `ELEPHC_TEST_TARGET` to cover the non-host architectures.
#[test]
fn test_byref_foreach_missing_source_emits_null_container_guards() {
    let dir = make_cli_test_dir("elephc_byref_foreach_null_guards");
    let (user_asm, runtime_asm, _libs) = compile_source_to_asm_with_options(
        r#"<?php
$a = [['x', 'y']];
foreach ($a[7] as &$v) { $v = 'changed'; }
echo 'done';
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    // -- runtime: both COW helpers bail on the sentinel before the refcount load --
    for helper in ["array_ensure_unique", "hash_ensure_unique"] {
        let start = runtime_asm
            .find(&format!("runtime: {helper}"))
            .unwrap_or_else(|| panic!("missing {helper} runtime section"));
        let section = &runtime_asm[start..];
        let end = section[10..]
            .find("--- runtime:")
            .map(|pos| pos + 10)
            .unwrap_or(section.len());
        let section = &section[..end];
        // Internal labels are `L`-prefixed on macOS only, so the bail branch is
        // matched as branch mnemonic + label suffix on one line instead of verbatim.
        let (sentinel_cmp, bail_branch, refcount_load) = match target().arch {
            Arch::AArch64 => ("cmp x0, x9", "b.eq", "[x0, #-12]"),
            Arch::X86_64 => ("cmp rdi, r10", "je", "[rdi - 12]"),
        };
        let done_label = format!("__rt_{helper}_done");
        let cmp_pos = section
            .find(sentinel_cmp)
            .unwrap_or_else(|| panic!("{helper}: missing sentinel compare:\n{section}"));
        // Scan for the bail line only after the sentinel compare: the plain null
        // check earlier in the helper branches to the same done label (on x86_64
        // with the same `je` mnemonic).
        let after_cmp = &section[cmp_pos..];
        let bail_pos = {
            let mut offset = cmp_pos;
            let mut found = None;
            for line in after_cmp.lines() {
                if line.contains(bail_branch) && line.contains(&done_label) {
                    found = Some(offset);
                    break;
                }
                offset += line.len() + 1;
            }
            found.unwrap_or_else(|| panic!("{helper}: missing sentinel bail branch:\n{section}"))
        };
        let refcount_pos = section
            .find(refcount_load)
            .unwrap_or_else(|| panic!("{helper}: missing refcount load:\n{section}"));
        assert!(
            cmp_pos < bail_pos && bail_pos < refcount_pos,
            "{helper}: sentinel guard must precede the refcount load:\n{section}"
        );
    }

    // -- IterStart: the sentinel source is folded to zero in the iterator slot --
    let normalize = match target().arch {
        Arch::AArch64 => "csel x0, xzr, x0, eq",
        Arch::X86_64 => "cmove rax, r10",
    };
    assert!(
        user_asm.contains(normalize),
        "missing iter_start sentinel-to-zero normalization:\n{user_asm}"
    );

    // -- IterNext: the by-reference live-length read keeps its zero-source guard --
    // Look for the label *definition* (a line ending with ':'), not the branch operand.
    assert!(
        user_asm
            .lines()
            .any(|line| line.trim_end().ends_with(':') && line.contains("iter_len_null_source")),
        "missing iter_next zero-length guard label:\n{user_asm}"
    );

    let _ = fs::remove_dir_all(&dir);
}

/// Regression for issue #556: a missed read assigned to a local and then iterated
/// by reference exercises the ensure-unique store-back path; the loop is skipped
/// and the origin local still carries its null marker afterwards (a botched fix
/// that normalized the local itself would make `??` keep an empty array instead).
#[test]
fn test_byref_foreach_over_missing_local_source_keeps_local_null() {
    let out = compile_and_run_capture(
        r#"<?php
$a = [['x', 'y']];
$arr = $a[7];
foreach ($arr as &$v) { $v = 'changed'; }
$probe = $arr ?? 'was-null';
echo is_array($probe) ? 'kept-array' : $probe;
echo '|done';
"#,
    );
    assert!(out.success, "program crashed: {}", out.stderr);
    assert_eq!(out.stdout, "was-null|done");
    assert!(out.stderr.contains("Warning: Undefined array key 7"));
}

/// Regression for issue #556: the keyed by-reference form over a missed element
/// skips the loop without binding a key or crashing.
#[test]
fn test_byref_foreach_keyed_over_first_index_miss_skips_loop() {
    let out = compile_and_run_capture(
        r#"<?php
$a = [['x', 'y']];
foreach ($a[7] as $k => &$v) { $v = 'changed'; echo 'k=' . $k; }
echo 'done';
"#,
    );
    assert!(out.success, "program crashed: {}", out.stderr);
    assert_eq!(out.stdout, "done");
    assert!(out.stderr.contains("Warning: Undefined array key 7"));
}

/// Guard for issue #556: by-reference foreach still reads the live array length,
/// so elements appended during iteration are visited exactly like PHP.
#[test]
fn test_byref_foreach_still_visits_elements_appended_during_iteration() {
    let out = compile_and_run_capture(
        r#"<?php
$a = [10, 20];
foreach ($a as &$v) {
    if (count($a) < 3) { $a[] = 30; }
    echo $v . ',';
}
echo 'done';
"#,
    );
    assert!(out.success, "program crashed: {}", out.stderr);
    assert_eq!(out.stdout, "10,20,30,done");
    assert_eq!(out.stderr, "");
}
