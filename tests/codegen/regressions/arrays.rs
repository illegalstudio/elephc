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

/// x86_64-only regression pin for `__rt_array_get_mixed_key`'s hash-storage branch
/// (`src/codegen_support/runtime/arrays/array_get_mixed_key.rs`,
/// `__rt_array_get_mixed_key_hash`). Mixing an int and a string value gives `$a` a
/// statically Mixed element type, and the string-keyed write promotes its runtime
/// storage from indexed (kind 2) to hash (kind 3) via `__rt_array_set_mixed_key`.
/// Reading it back through a non-literal string key routes through
/// `Op::ArrayGetMixedKey`, whose x86_64 hash branch previously read `__rt_hash_get`'s
/// real return registers (`rax`=found, `rdi`=value_lo, `rsi`=value_hi, `rcx`=value_tag)
/// as if they mirrored the ARM64 convention (`rsi`=value_lo, `rdx`=value_hi,
/// `rcx`=value_tag) — `rdx` was never set by `__rt_hash_get`, so a garbage pointer got
/// boxed into the returned `Mixed` cell and SIGSEGV'd on first deref. The ARM64 branch
/// was always correct, so this test is only meaningful — and only regresses — on
/// x86_64.
#[test]
fn test_array_get_mixed_key_hash_storage_string_key_roundtrip() {
    let out = compile_and_run(
        r#"<?php
$a = [];
$a[0] = 1;
$a["greeting"] = "hello world";
$key = "greeting";
echo $a[$key];
"#,
    );
    assert_eq!(out, "hello world");
}

// --- isset()/array_key_exists() on an `Array(_)`-typed receiver with a Str/Mixed key ---
//
// `isset($arr[$strKey])` and `array_key_exists($strKey, $arr)` used to fail (or, for
// `isset`, silently miscompile — see below) whenever `$arr` was still statically an
// indexed array type, which is the normal state for a membership probe done before the
// array's first string-keyed write. The checker only promotes an array's *static* type
// to `PhpType::AssocArray` at a provably string-keyed write
// (`src/types/checker/stmt_check/assignments/arrays.rs`), so a probe beforehand — or an
// array that only ever gets a *dynamic* (Mixed-typed key) write, which the checker can't
// prove is string-keyed — keeps the `Array(_)` type through to codegen.
//
// All of these fixtures route the key through a helper function parameter (not a local
// reassigned from a literal in the same straight-line block) because `elephc`'s AST-level
// constant-propagation pass (`src/optimize/propagate`) folds a straight-line
// `$k = "foo"; ...$arr[$k]...` back into a literal `$arr["foo"]` — which already worked
// before this fix, since `normalized_array_key_type` special-cases literal expressions.
// Routing through a function parameter is what actually exercises a *non-literal* Str/
// Mixed key and would have hit the bugs below without it.

/// Verifies `isset($arr[$k])` and `array_key_exists($k, $arr)` compile and both answer
/// `false` for a non-literal string key probed on a plain indexed array *before* any
/// string-keyed write ever happened — the original failing shape (A1/A3). Before the
/// fix, `array_key_exists` hit a loud compile error ("array_key_exists key PHP type
/// Str") from `require_indexed_key_type` in
/// `src/codegen/lower_inst/builtins/arrays/key_exists.rs`, and `isset` — once its
/// receiver-type gate was already permissive enough to reach
/// `lower_native_isset_offset_probe_from_value` — silently miscompiled: `index_expr_key_type`
/// (`src/ir_lower/expr/mod.rs`) derived the key's type from `infer_expr_type_syntactic`,
/// which has no `ExprKind::Variable` arm and defaults to `PhpType::Int`, so the string key
/// got `str_to_i`-coerced (`"foo"` → `0`) and probed integer index 0 instead.
#[test]
fn test_indexed_array_string_key_probe_before_any_hash_write() {
    let out = compile_and_run(
        r#"<?php
function isset_of(array $arr, string $k): bool {
    return isset($arr[$k]);
}
function key_exists_of(array $arr, string $k): bool {
    return array_key_exists($k, $arr);
}
$arr = [10, 20, 30];
echo isset_of($arr, "foo") ? "yes" : "no";
echo ":";
echo key_exists_of($arr, "foo") ? "yes" : "no";
"#,
    );
    assert_eq!(out, "no:no");
}

/// Verifies a packed/indexed array (never promoted to hash storage) probed with a
/// non-literal string key returns `false` for both constructs without crashing, and
/// without corrupting the result the way the pre-fix `index_expr_key_type` default did
/// (`str_to_i("bogus")` → `0`, which is in-bounds on a 2-element array and would have
/// wrongly reported both constructs as `true`).
#[test]
fn test_packed_indexed_array_string_key_probe_no_hash_promotion() {
    let out = compile_and_run_capture(
        r#"<?php
function isset_of(array $arr, string $k): bool {
    return isset($arr[$k]);
}
function key_exists_of(array $arr, string $k): bool {
    return array_key_exists($k, $arr);
}
$arr = ["a", "b"];
var_dump(isset_of($arr, "bogus"));
var_dump(key_exists_of($arr, "bogus"));
"#,
    );
    assert!(out.success);
    assert_eq!(out.stdout, "bool(false)\nbool(false)\n");
}

/// Verifies `isset($arr[$k])` answers `true` for a present, non-null value and `false`
/// for an absent key, once `$arr` has been promoted to runtime hash storage via a
/// dynamic (Mixed-typed key) write that the checker cannot statically prove is
/// string-keyed — so `$arr` stays `Array(Mixed)`, not `AssocArray`, and the probe must
/// go through `__rt_array_get_mixed_key`'s runtime storage-kind dispatch.
#[test]
fn test_indexed_array_mixed_key_write_then_isset_present_and_absent() {
    let out = compile_and_run_capture(
        r#"<?php
function set_key(array $arr, mixed $k, mixed $v): array {
    $arr[$k] = $v;
    return $arr;
}
function isset_of(array $arr, string $k): bool {
    return isset($arr[$k]);
}
$a = set_key([], "present", "value");
var_dump(isset_of($a, "present"));
var_dump(isset_of($a, "absent"));
"#,
    );
    assert!(out.success);
    assert_eq!(out.stdout, "bool(true)\nbool(false)\n");
}

/// The distinguishing semantic pair (per PHP): `array_key_exists()` is `true` for a key
/// whose stored value is `null`, while `isset()` is `false` for that same key (isset
/// requires present *and* non-null). This is why `array_key_exists`'s mixed-key runtime
/// path is a dedicated presence-only helper (`__rt_array_key_exists_mixed_key`,
/// `src/codegen_support/runtime/arrays/array_key_exists_mixed_key.rs`) rather than a
/// reuse of `__rt_array_get_mixed_key` plus an is-null check (which is exactly how
/// `isset`'s own probe is built, and exactly why it must answer differently here) — a
/// design that answers `array_key_exists` from `is_null` would get this pair backwards.
#[test]
fn test_indexed_array_mixed_key_null_value_distinguishes_key_exists_from_isset() {
    let out = compile_and_run_capture(
        r#"<?php
function set_key(array $arr, mixed $k, mixed $v): array {
    $arr[$k] = $v;
    return $arr;
}
function isset_of(array $arr, string $k): bool {
    return isset($arr[$k]);
}
function key_exists_of(array $arr, string $k): bool {
    return array_key_exists($k, $arr);
}
$a = set_key([], "k", null);
var_dump(key_exists_of($a, "k"));
var_dump(isset_of($a, "k"));
"#,
    );
    assert!(out.success);
    assert_eq!(out.stdout, "bool(true)\nbool(false)\n");
}

/// Verifies `array_key_exists($k, $arr)` answers `true`/`false` correctly for a
/// non-literal string key once `$arr` has been promoted to runtime hash storage via a
/// dynamic mixed-key write, while `$arr` stays statically `Array(Mixed)` (not
/// `AssocArray`) — the gap this test targets in
/// `src/codegen/lower_inst/builtins/arrays/key_exists.rs::lower_indexed_array_key_exists`.
#[test]
fn test_indexed_array_mixed_key_write_then_key_exists_present_and_absent() {
    let out = compile_and_run_capture(
        r#"<?php
function set_key(array $arr, mixed $k, mixed $v): array {
    $arr[$k] = $v;
    return $arr;
}
function key_exists_of(array $arr, string $k): bool {
    return array_key_exists($k, $arr);
}
$a = set_key([], "present", 1);
var_dump(key_exists_of($a, "present"));
var_dump(key_exists_of($a, "absent"));
"#,
    );
    assert!(out.success);
    assert_eq!(out.stdout, "bool(true)\nbool(false)\n");
}

/// Verifies `isset($arr[$intKey])` and `array_key_exists($intKey, $arr)` stay correct once
/// `$arr` has been promoted to runtime *hash* storage by a mixed-key write, while its static
/// type remains an indexed `Array(Mixed)`.
///
/// The mixed-key probes dispatch on the runtime storage kind, but the integer-key probes used
/// to be storage-kind-blind: they bounds-checked the index against the array header's first
/// word — which on hash storage is the live-entry COUNT, not a length — and then read
/// `[array + 24 + 8 * idx]` as if it were a packed element. On a hash header offset 24 holds
/// `head`, an insertion-order slot index, so `isset($a[0])` on a one-entry hash both passed the
/// bounds check and dereferenced that integer as a boxed `Mixed` pointer (SIGSEGV whenever
/// `head != 0`), and `array_key_exists(0, $a)` answered `true` for a key that does not exist.
#[test]
fn test_indexed_array_int_key_probe_after_runtime_hash_promotion() {
    let out = compile_and_run_capture(
        r#"<?php
function set_key(array $arr, mixed $k, mixed $v): array {
    $arr[$k] = $v;
    return $arr;
}
function isset_of(array $arr, int $k): bool {
    return isset($arr[$k]);
}
function key_exists_of(array $arr, int $k): bool {
    return array_key_exists($k, $arr);
}
$a = set_key([], "present", "value");
var_dump(isset_of($a, 0));
var_dump(key_exists_of($a, 0));
$b = set_key([], 7, "seven");
var_dump(isset_of($b, 7));
var_dump(key_exists_of($b, 7));
var_dump(isset_of($b, 0));
var_dump(key_exists_of($b, 0));
"#,
    );
    assert!(out.success);
    assert_eq!(
        out.stdout,
        "bool(false)\nbool(false)\nbool(true)\nbool(true)\nbool(false)\nbool(false)\n"
    );
}

/// Verifies PHP's numeric-string array-key coercion holds through the mixed-key probes on
/// packed storage: `"1"` is the *integer* key 1 (present), while `"0123"`, `"-1"`, `"3"` and
/// `"foo"` are all absent — `"0123"` because a leading zero makes it a genuine string key,
/// `"-1"` and `"3"` because they coerce to out-of-range integer keys.
///
/// This is the only test that reaches the new helper's packed-storage *found* branch: every
/// other packed probe in this file asserts absence, so a helper that always answered
/// "not found" on kind-2 storage would otherwise still be green.
#[test]
fn test_packed_indexed_array_numeric_string_key_probes_coerce_like_php() {
    let out = compile_and_run_capture(
        r#"<?php
function isset_of(array $arr, string $k): bool {
    return isset($arr[$k]);
}
function key_exists_of(array $arr, string $k): bool {
    return array_key_exists($k, $arr);
}
$p = [10, 20, 30];
foreach (["1", "0123", "-1", "3", "foo"] as $k) {
    echo $k, "=", isset_of($p, $k) ? "1" : "0", key_exists_of($p, $k) ? "1" : "0", "\n";
}
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "1=11\n0123=00\n-1=00\n3=00\nfoo=00\n");
}

/// Verifies a `Mixed`-typed key that holds a runtime *float* truncates toward zero into an
/// integer key, exactly as PHP casts a float array key — `isset($a[2.7])` finds key 2.
///
/// `materialize_mixed_hash_key_{aarch64,x86_64}` had no float arm, so tag 2 fell into the
/// "unsupported tag" fallback and normalized to integer key **0**, silently probing the
/// wrong slot. The same test also pins a `Mixed` key holding a runtime *string*, which is
/// the case the isset probe's `Mixed`/`Union` widening exists for in the first place.
#[test]
fn test_mixed_key_holding_float_truncates_and_holding_string_resolves() {
    let out = compile_and_run_capture(
        r#"<?php
function set_key(array $arr, mixed $k, mixed $v): array {
    $arr[$k] = $v;
    return $arr;
}
function isset_m(array $arr, mixed $k): bool {
    return isset($arr[$k]);
}
function key_exists_m(array $arr, mixed $k): bool {
    return array_key_exists($k, $arr);
}
$a = set_key([], 2, "two");
var_dump(isset_m($a, 2.7));
$b = set_key([], "s", "v");
var_dump(isset_m($b, "s"));
var_dump(isset_m($b, "nope"));
var_dump(key_exists_m($b, "s"));
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "bool(true)\nbool(true)\nbool(false)\nbool(true)\n"
    );
}

/// Verifies the `array_key_exists`-vs-`isset` split for a null *element of packed storage*.
///
/// The sibling test above pins this on hash storage, which takes `__rt_hash_get`'s found
/// flag; packed storage takes a completely different branch (a bounds check plus an element
/// null probe), so it needs its own coverage: a null element is **present** for
/// `array_key_exists` and **absent** for `isset`.
#[test]
fn test_packed_indexed_array_null_element_distinguishes_key_exists_from_isset() {
    let out = compile_and_run_capture(
        r#"<?php
function isset_i(array $arr, int $k): bool {
    return isset($arr[$k]);
}
function key_exists_i(array $arr, int $k): bool {
    return array_key_exists($k, $arr);
}
$p = [1, null, 3];
var_dump(key_exists_i($p, 1));
var_dump(isset_i($p, 1));
var_dump(key_exists_i($p, 9));
var_dump(isset_i($p, 9));
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "bool(true)\nbool(false)\nbool(false)\nbool(false)\n"
    );
}

/// Verifies `isset()` stays silent on a missing mixed key while a plain read of the same
/// missing key still warns — the two probes deliberately lower to different ops
/// (`Op::ArrayGetMixedKeySilent` vs `Op::ArrayGetMixedKey`), and nothing else pins that
/// split, so a future refactor that unified them would go unnoticed.
#[test]
fn test_isset_mixed_key_is_silent_while_read_of_missing_key_warns() {
    let quiet = compile_and_run_capture(
        r#"<?php
function set_key(array $arr, mixed $k, mixed $v): array {
    $arr[$k] = $v;
    return $arr;
}
function isset_of(array $arr, string $k): bool {
    return isset($arr[$k]);
}
$a = set_key([], "present", "value");
var_dump(isset_of($a, "absent"));
"#,
    );
    assert!(quiet.success, "program failed: {}", quiet.stderr);
    assert_eq!(quiet.stdout, "bool(false)\n");
    assert!(
        !quiet.stderr.contains("Undefined array key"),
        "isset() must never emit an undefined-array-key warning, got: {}",
        quiet.stderr
    );

    let noisy = compile_and_run_capture(
        r#"<?php
function set_key(array $arr, mixed $k, mixed $v): array {
    $arr[$k] = $v;
    return $arr;
}
function read_of(array $arr, string $k): mixed {
    return $arr[$k];
}
$a = set_key([], "present", "value");
var_dump(read_of($a, "absent"));
"#,
    );
    assert!(noisy.success, "program failed: {}", noisy.stderr);
    assert!(
        noisy.stderr.contains("Undefined array key"),
        "a plain read of a missing key must still warn, got: {}",
        noisy.stderr
    );
}

/// Verifies a nullable-int key still compiles and probes correctly.
///
/// A `?int` funnels to the codegen-internal `TaggedScalar` repr — an inline `{payload, tag}`
/// pair, not a boxed `Mixed` cell — which the mixed-key codegen has no arm for. Widening the
/// `isset` key-type upgrade to unions therefore has to exclude it explicitly, or this valid
/// PHP stops compiling.
#[test]
fn test_isset_with_nullable_int_key_stays_on_the_integer_path() {
    let out = compile_and_run_capture(
        r#"<?php
function isset_of(array $arr, ?int $k): bool {
    return isset($arr[$k]);
}
$p = [10, 20, 30];
var_dump(isset_of($p, 1));
var_dump(isset_of($p, 7));
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "bool(true)\nbool(false)\n");
}

/// Verifies the mixed-key probes and reads do not leak *per evaluation*.
///
/// `__rt_array_get_mixed_key` hands back an *owned* Mixed cell (it boxes the element into a
/// fresh cell — even a miss returns a boxed null — or increfs an already-boxed one), so both
/// the `isset` probe and a plain `$arr[$key]` read own a reference. Neither op was classified
/// as an owning container read, and the probe stored its cell nowhere, so every evaluation
/// leaked one cell.
///
/// The assertion is deliberately a *scaling* one rather than `leak summary: clean`: the
/// mixed-key **write** that builds the fixture (`$arr[$k] = $v`) has its own pre-existing
/// per-call leak, which a clean-heap assertion would trip over and which would mask this
/// test's actual subject. Running the identical program at two loop counts holds that
/// constant baseline fixed, so any residual growth is attributable to the probes and reads
/// alone — a per-evaluation leak shows up as a difference proportional to the extra
/// iterations, and nothing else can.
#[test]
fn test_mixed_key_probe_and_read_do_not_leak_per_evaluation() {
    fn live_blocks(iterations: usize) -> u64 {
        let source = format!(
            r#"<?php
function set_key(array $arr, mixed $k, mixed $v): array {{
    $arr[$k] = $v;
    return $arr;
}}
function isset_of(array $arr, string $k): bool {{
    return isset($arr[$k]);
}}
function read_of(array $arr, string $k): mixed {{
    return $arr[$k];
}}
$a = set_key([], "present", "value");
$hits = 0;
for ($i = 0; $i < {}; $i++) {{
    if (isset_of($a, "present")) {{
        $hits++;
    }}
    if (isset_of($a, "absent")) {{
        $hits++;
    }}
    $v = read_of($a, "present");
}}
echo $hits;
"#,
            iterations
        );
        let out = compile_and_run_with_heap_debug(&source);
        assert!(out.success, "program failed: {}", out.stderr);
        assert_eq!(out.stdout, iterations.to_string());
        let summary = out
            .stderr
            .lines()
            .find(|line| line.contains("leak summary"))
            .unwrap_or_else(|| panic!("no heap-debug leak summary in: {}", out.stderr))
            .to_string();
        if summary.contains("clean") {
            return 0;
        }
        summary
            .split("live_blocks=")
            .nth(1)
            .and_then(|rest| rest.split_whitespace().next())
            .and_then(|count| count.parse().ok())
            .unwrap_or_else(|| panic!("unparsable heap-debug leak summary: {}", summary))
    }

    let few = live_blocks(8);
    let many = live_blocks(256);
    assert_eq!(
        few, many,
        "mixed-key probes/reads leak per evaluation: 8 iterations left {} live blocks and \
         256 iterations left {} — the growth scales with the loop, so each probe or read is \
         leaking the boxed Mixed cell it owns",
        few, many
    );
}

/// Verifies a nested append through a *static property* actually appends.
///
/// `self::$b[$k][] = $v` was a silent miscompile: the append was dropped and the bucket was
/// OVERWRITTEN with the single value, i.e. it compiled as `self::$b[$k] = $v`. A statement
/// whose LHS contains `::` is claimed by `try_parse_scoped_property_assignment`, which strips
/// the trailing `[]` before parsing the target — so the parsed target is an `ExprKind::ArrayAccess`
/// and the `if is_append` guard, which sits only on the bare `StaticPropertyAccess` arm
/// (handling `self::$b[] = $v`), can never match it. The plain `ArrayAccess` arm that caught it
/// instead ignored `is_append` entirely. Nothing downstream recovered the append, and no test
/// covered the shape.
#[test]
fn test_static_property_nested_append_actually_appends() {
    let out = compile_and_run_capture(
        r#"<?php
class Bag {
    public static array $b = [];

    public static function add(int $v): void {
        self::$b[0][] = $v;
    }
}
Bag::$b[0] = [];
Bag::add(1);
Bag::add(2);
Bag::add(3);
echo count(Bag::$b[0]), ":", Bag::$b[0][0], Bag::$b[0][1], Bag::$b[0][2], "\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "3:123\n");
}

/// Verifies PHP's auto-vivification of a nested append into a key that does not exist yet.
///
/// `$g = []; $g["k"][] = 1;` used to print `count() == 0`: the parser desugars every nested
/// append into a read/push/write-back triple, and nothing created the missing inner array the
/// way PHP does. The read of a missing key came back as a boxed null, and the append then
/// silently DROPPED the value. This is the single most common grouping idiom in PHP, and it
/// lost every row of every new bucket.
#[test]
fn test_nested_append_auto_vivifies_a_missing_bucket() {
    let out = compile_and_run_capture(
        r#"<?php
$g = [];
$g["k"][] = 1;
$g["k"][] = 2;
$g["j"][] = 9;
echo count($g), "|", count($g["k"]), "|", $g["k"][0], $g["k"][1], "|", count($g["j"]), "|", $g["j"][0], "\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "2|2|12|1|9\n");
}

/// Negative control for the nested-append fusion: copy-on-write must still fire when the bucket
/// is genuinely shared.
///
/// The fusion makes the append mutate the bucket *in place* by nulling the container slot between
/// the read and the push, so the bucket's refcount drops to one. That is only sound because a
/// bucket the user also holds a reference to still has a count above one, and copy-on-write still
/// splits it. If this test ever prints `2:2` instead of `1:2`, the fusion has silently broken PHP
/// value semantics — which no amount of speed would justify.
#[test]
fn test_nested_append_still_copies_a_shared_bucket() {
    let out = compile_and_run_capture(
        r#"<?php
$a = ["k" => [1]];
$c = $a["k"];
$a["k"][] = 2;
echo count($c), ":", count($a["k"]), "\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "1:2\n");
}

/// Complexity gate: appending into one bucket must allocate a number of blocks that grows
/// *linearly* with the number of rows.
///
/// Before the fusion, the read left the bucket owned twice (the container slot and the append
/// temporary), so every push copy-on-write cloned the whole bucket — O(length) per push, O(n^2)
/// overall. The assertion is deliberately a ratio rather than an absolute bound: one allocation
/// per push is inherent (each value is boxed into the bucket's Mixed storage), so any fixed
/// ceiling would fail on a correct build. Quadratic growth would show up as roughly a 16x
/// increase for 4x the rows; linear growth stays near 4x.
#[test]
fn test_nested_append_into_one_bucket_is_linear_not_quadratic() {
    fn allocs(rows: usize) -> u64 {
        let source = format!(
            r#"<?php
$g = [];
for ($i = 0; $i < {}; $i++) {{
    $g["k"][] = $i;
}}
echo count($g["k"]);
"#,
            rows
        );
        let out = compile_and_run_with_gc_stats(&source);
        assert!(out.success, "program failed: {}", out.stderr);
        assert_eq!(out.stdout, rows.to_string());
        let (allocs, frees) = parse_gc_stats(&out.stderr);
        assert_eq!(
            allocs, frees,
            "nested append leaks: {} allocs vs {} frees at {} rows",
            allocs, frees, rows
        );
        allocs
    }

    let small = allocs(250);
    let large = allocs(1000);
    assert!(
        large < small * 5,
        "nested append is super-linear: 250 rows allocated {} blocks, 1000 rows allocated {} \
         (a linear build lands near 4x; a copy-on-write clone per push lands near 16x)",
        small,
        large
    );
}

/// Verifies a mixed-key write into a still-typed indexed array does not destroy the array.
///
/// `$a[$k] = $v` with a `mixed` key routes to `__rt_array_set_mixed_key`. For an in-bounds
/// integer key that lands on packed storage, it called `__rt_array_set_mixed`, which re-stamps the
/// destination's `value_type` to 7 (boxed Mixed) and its slot width to 8 — but never converted the
/// slots ALREADY in the array. So `[1, 2, 3]` came back with slot 0 holding a real Mixed cell and
/// slots 1 and 2 still holding the raw integers 2 and 3, behind a header claiming all three were
/// cell pointers. Reading `$a[1]` dereferenced the integer `2` as an address and **segfaulted**;
/// only the element just written could be read back.
///
/// The key's static type is the whole trigger: a `mixed` key corrupts, an `int` key does not. The
/// fix widens the destination through `__rt_array_to_mixed` before the write, exactly as the
/// Mixed-append lowering already did.
#[test]
fn test_mixed_key_write_into_typed_indexed_array_preserves_the_other_elements() {
    let out = compile_and_run_capture(
        r#"<?php
function set_at(array $a, mixed $k, mixed $v): array {
    $a[$k] = $v;
    return $a;
}
function set_at_str(array $a, mixed $k, mixed $v): array {
    $a[$k] = $v;
    return $a;
}
$ints = set_at([1, 2, 3], 0, 99);
echo $ints[0], ",", $ints[1], ",", $ints[2], "\n";

$strs = set_at_str(["a", "b", "c"], 0, "z");
echo $strs[0], ",", $strs[1], ",", $strs[2], "\n";

$mid = set_at([1, 2, 3, 4, 5], 2, 77);
echo $mid[0], ",", $mid[1], ",", $mid[2], ",", $mid[3], ",", $mid[4], "\n";

$appended = set_at([1, 2], 2, 3);
echo count($appended), ":", $appended[0], $appended[1], $appended[2], "\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "99,2,3\nz,b,c\n1,2,77,4,5\n3:123\n"
    );
}

/// Negative control for the widening fix above: the caller's array must not be mutated.
///
/// A mixed-key write into an array the caller still holds has to copy-on-write split it, exactly
/// as PHP's by-value array semantics require. Widening the destination to boxed Mixed slots must
/// not turn that split into an in-place rewrite of the caller's own storage.
#[test]
fn test_mixed_key_write_still_copies_an_aliased_source_array() {
    let out = compile_and_run_capture(
        r#"<?php
function set_at(array $a, mixed $k, mixed $v): array {
    $a[$k] = $v;
    return $a;
}
$src = [1, 2, 3];
$copy = set_at($src, 0, 99);
echo $src[0], $src[1], $src[2], ":", $copy[0], $copy[1], $copy[2], "\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "123:9923\n");
}

/// Verifies PHP's by-value array parameter semantics: mutating an `array` parameter must NOT
/// mutate the caller's array.
///
/// elephc passed array arguments as a `+0` BORROW: the parameter slot held the caller's pointer
/// without ever incrementing its refcount, so `__rt_array_ensure_unique` — which only splits at
/// refcount >= 2 — stayed inert and every write in the callee landed straight in the CALLER's
/// storage. `function f(array $a) { $a[] = 1; }` modified the caller's array, on all three write
/// paths (mixed key, int key, append).
///
/// Each caller-side assertion below LAUNDERS its read through a function call. That is not
/// stylistic: a direct `$src[0]` after the call is CONSTANT-FOLDED — the optimizer folds the
/// literal, correctly assuming by-value semantics — so it prints the right answer while the
/// runtime mutates underneath. A test written with direct reads passes against the bug.
#[test]
fn test_array_parameters_are_passed_by_value() {
    let out = compile_and_run_capture(
        r#"<?php
function set_mixed(array $a, mixed $k, mixed $v): array {
    $a[$k] = $v;
    return $a;
}
function set_int(array $a, int $k, int $v): array {
    $a[$k] = $v;
    return $a;
}
function push_it(array $a, int $v): array {
    $a[] = $v;
    return $a;
}
function first_of(array $a): int {
    return $a[0];
}
function len_of(array $a): int {
    return count($a);
}

$m = [1, 2, 3];
$rm = set_mixed($m, 0, 99);
echo first_of($m), ":", first_of($rm), "\n";

$i = [1, 2, 3];
$ri = set_int($i, 0, 99);
echo first_of($i), ":", first_of($ri), "\n";

$p = [1, 2, 3];
$rp = push_it($p, 4);
echo len_of($p), ":", len_of($rp), "\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "1:99\n1:99\n3:4\n");
}

/// Verifies the by-value parameter privatization keeps the refcount ledger balanced.
///
/// The privatized shadow slot holds a genuine `+1`, consumed either by the copy-on-write split's
/// raw decrement or by the epilogue release — never both, never neither. Two shapes used to die
/// outright with `heap debug detected bad refcount` (a use-after-free): a parameter merely READ in
/// a loop (an inliner bug — its cleanup exclusion was a no-op, so the host released a borrow it
/// never acquired), and a parameter mutated and returned.
#[test]
fn test_array_parameter_passing_keeps_the_refcount_ledger_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function read_only(array $a): int {
    return count($a);
}
function mutate_and_return(array $a): array {
    $a[] = 9;
    return $a;
}
function mutate_and_discard(array $a): int {
    $a[] = 9;
    return count($a);
}
$src = [1, 2, 3];
$total = 0;
for ($i = 0; $i < 8; $i++) {
    $total += read_only($src);
    $kept = mutate_and_return($src);
    $total += mutate_and_discard($src);
    $fresh = mutate_and_return([7, 8]);
    $total += count($kept) + count($fresh);
}
echo $total, ":", count($src);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "112:3");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap, got: {}",
        out.stderr
    );
}

/// Verifies a generic `array` parameter hint joins over ALL its call sites, not just the first.
///
/// A bare `array` hint resolves to `Array(Mixed)` and is then specialized to the first call site's
/// concrete element type. That narrowing was never joined with later calls, and the widening
/// machinery that exists for exactly this (`union_param_type`) was gated on `!declared_params[i]`
/// — which excludes `array`, because `array` is, technically, declared. So `first_of` pinned to
/// `Array(Int)` on its first call, its body was code-generated for raw integer slots, and a later
/// `Array(Mixed)` argument (whose slots hold boxed cell POINTERS) was accepted silently and read
/// back as a raw scalar: it printed a pointer.
///
/// The tell was order-dependence — calling `first_of($mixed)` FIRST made both calls correct. This
/// test pins the buggy order.
#[test]
fn test_generic_array_param_widens_across_call_sites() {
    let out = compile_and_run_capture(
        r#"<?php
function widen(array $a, mixed $k, mixed $v): array {
    $b = $a;
    $b[$k] = $v;
    return $b;
}
function first_of(array $a): int {
    return $a[0];
}
$ints = [1, 2, 3];
$mixed = widen($ints, 0, 99);
echo first_of($ints), ":", first_of($mixed), "\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "1:99\n");
}

/// Verifies a plain read `$a[$k]` on an `Array(_)` receiver promoted to hash storage does not
/// crash.
///
/// `Op::ArrayGet`'s codegen walked the packed payload unconditionally. An `Array(_)`-typed local
/// can be HASH-backed at runtime — a mixed-key write promotes the storage kind while the checker
/// only promotes the static type to `AssocArray` at a provably string-keyed write — so the read
/// bounds-checked the index against the hash header's live-entry COUNT and then read the header's
/// own fields as if they were elements. It SEGFAULTED. `Op::ArrayIsset` and `array_key_exists`
/// were given a storage-kind dispatch; `Op::ArrayGet` was not, and it is the one shape that
/// actually crashes.
#[test]
fn test_array_read_on_a_hash_promoted_receiver_does_not_crash() {
    let out = compile_and_run_capture(
        r#"<?php
function set_key(array $a, mixed $k, mixed $v): array {
    $a[$k] = $v;
    return $a;
}
function read_at(array $a, mixed $k): mixed {
    return $a[$k];
}
$promoted = set_key([], "foo", 42);
var_dump(read_at($promoted, 0));
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "NULL\n");
}

/// Verifies a `Mixed`-typed key reads the RIGHT element, on both storage kinds.
///
/// `$a[$k]` with a `mixed` key used to `str_to_i`-coerce the key before the read: `"foo"` became
/// `0` and the read silently returned element 0. The obvious fix — routing such a key to the
/// mixed-key op — was NOT taken, because that op retypes the read's RESULT to a boxed `Mixed`,
/// which breaks a downstream typed assignment like `Iterator $it = $this->iterators[$i]`. And that
/// is not a corner case: `$i++` lowers to `Op::IChecked*`, whose PHP result type is `Mixed`, so
/// EVERY incremented loop counter is statically `Mixed` from its second use on. (Widening it
/// naively regressed four `MultipleIterator` tests.)
///
/// Instead the key keeps `Op::ArrayGet` — whose result stays the array's element type — and is
/// simply no longer coerced; the codegen materializes it on both storage kinds. PHP's
/// numeric-string key rule then comes for free from `materialize_hash_key`: `"1"` IS the integer
/// key 1, while `"foo"` is a genuine string key, which never exists in packed storage.
#[test]
fn test_mixed_typed_key_reads_the_right_element() {
    let out = compile_and_run_capture(
        r#"<?php
function set_key(array $a, mixed $k, mixed $v): array {
    $a[$k] = $v;
    return $a;
}
function read_at(array $a, mixed $k): mixed {
    return $a[$k];
}
$promoted = set_key([], "foo", 42);
var_dump(read_at($promoted, "foo"));

$packed = [10, 20, 30];
var_dump(read_at($packed, "1"));
var_dump(read_at($packed, 2));
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "int(42)\nint(20)\nint(30)\n");
}

/// Verifies nested append auto-vivifies on an OBJECT-PROPERTY base, not just a local.
///
/// `$this->b[$k][] = $v` desugars to the same read/push/write-back triple as a local base, and hit
/// the same defect: nothing created the missing inner array, so the first push into every bucket
/// read back a boxed null and silently DROPPED the value. `count()` returned 0.
///
/// The fusion vivifies through the ordinary `PropertyArrayAssign` lowering — the very one the
/// group's own write-back uses — because the append temporary's checker type is the container's
/// VALUE type (typically `Mixed`), so assigning a bare `Array(Never)` literal into it would bypass
/// the boxing the storage expects and the bucket would segfault the moment it outgrew its initial
/// capacity. The growth case below pins exactly that.
///
/// A property base deliberately does NOT get `Op::SlotDetach`: that op republishes the possibly
/// rehashed container pointer through a LOCAL slot, and on a property the new pointer would never
/// reach the property. So the property base is correct but still quadratic.
#[test]
fn test_nested_append_vivifies_on_a_property_base() {
    let out = compile_and_run_capture(
        r#"<?php
class Bag {
    public array $b = [];

    public function add(string $k, int $v): void {
        $this->b[$k][] = $v;
    }
}
$bag = new Bag();
$bag->add("a", 1);
$bag->add("b", 2);
$bag->add("a", 3);
echo count($bag->b), ":", count($bag->b["a"]), ":", count($bag->b["b"]), "\n";

$grow = new Bag();
for ($i = 0; $i < 12; $i++) {
    $grow->add("k", $i);
}
echo count($grow->b["k"]), ":", $grow->b["k"][11], "\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "2:2:1\n12:11\n");
}

/// Verifies nested append auto-vivifies on a STATIC-property base.
///
/// This shape needed two separate fixes. The parser was dropping the append outright —
/// `self::$b[$k][] = $v` compiled as `self::$b[$k] = $v`, overwriting the bucket — and once that
/// was routed through the desugar it inherited the missing auto-vivification and lost the first
/// row of every bucket instead. Both are fixed; this pins the pair.
#[test]
fn test_nested_append_vivifies_on_a_static_property_base() {
    let out = compile_and_run_capture(
        r#"<?php
class Registry {
    public static array $b = [];

    public static function add(int $k, int $v): void {
        self::$b[$k][] = $v;
    }
}
Registry::add(0, 1);
Registry::add(0, 2);
Registry::add(1, 9);
echo count(Registry::$b), ":", count(Registry::$b[0]), ":", count(Registry::$b[1]), "\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "2:2:1\n");
}

/// Negative control: a nested append on a property base must still copy-on-write a shared bucket.
#[test]
fn test_nested_append_on_a_property_base_still_copies_a_shared_bucket() {
    let out = compile_and_run_capture(
        r#"<?php
class Bag {
    public array $b = [];
}
$bag = new Bag();
$bag->b["k"] = [1];
$snapshot = $bag->b["k"];
$bag->b["k"][] = 2;
echo count($snapshot), ":", count($bag->b["k"]), "\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "1:2\n");
}

/// Verifies a store into a `Mixed`-typed property BOXES its value.
///
/// An UNTYPED property becomes `Mixed` as soon as two different types are assigned to it (here
/// `$this->pos = 0` and `$this->pos = $this->pos + 1`). A `Mixed` slot holds a boxed cell — but
/// `Op::PropSet` was handed the value exactly as lowered, so `$this->pos = 0` wrote a RAW integer
/// into it, and reading it back dereferenced that integer as a cell pointer. `var_dump($this->pos)`
/// printed **NULL** right after assigning `0` to it.
///
/// This bug survived because ANOTHER bug masked it perfectly: the array-read path used to coerce a
/// `Mixed` key with `__rt_mixed_cast_int`, and casting that null cell gives `0` — so `$names[$this->pos]`
/// read element 0 and returned the right answer, by accident. Reading the key correctly is what
/// exposed it. Two bugs propping each other up.
#[test]
fn test_store_into_a_mixed_typed_property_is_boxed() {
    let out = compile_and_run_capture(
        r#"<?php
class Cursor {
    public $pos = 0;

    public function reset(): void {
        $this->pos = 0;
    }

    public function bump(): void {
        $this->pos = $this->pos + 1;
    }
}
$c = new Cursor();
var_dump($c->pos);
$c->reset();
var_dump($c->pos);
$c->bump();
var_dump($c->pos);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "int(0)\nint(0)\nint(1)\n");
}

/// The end-to-end shape the two bugs above conspired to hide: an untyped cursor property used as an
/// array index, advanced across calls. It is exactly `dir_readdir()`'s body in a stream wrapper.
#[test]
fn test_untyped_cursor_property_indexes_an_array_across_calls() {
    let out = compile_and_run_capture(
        r#"<?php
class Reader {
    public $pos = 0;

    public function next(): string {
        $names = ["a.txt", "b.txt"];
        if ($this->pos >= 2) {
            return "";
        }
        $n = $names[$this->pos];
        $this->pos = $this->pos + 1;
        return $n;
    }
}
$r = new Reader();
echo $r->next(), "|", $r->next(), "|", $r->next(), "|\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "a.txt|b.txt||\n");
}

/// A nullable-int array element (`?int` — `TaggedScalar`) has NO hash representation: neither
/// `hash_get` nor `hash_set` can materialize one. The storage-kind dispatch added to `ArrayGet`
/// emits its promoted-hash branch *speculatively* (an `Array(_)` local may be hash-backed at
/// runtime), and emitting it for such an element type does not sit unreached — it fails the whole
/// compilation with `unsupported EIR backend feature: hash_get value PHP type TaggedScalar`.
///
/// This shape lives in `ir_backend_smoke_test`, a binary the codegen suite does not cover, so it
/// went unseen until the x86_64 run. Anchor it here, in the suite that is actually run.
#[test]
fn test_nullable_int_array_elements_still_compile_after_kind_dispatch() {
    let out = compile_and_run_capture(
        r#"<?php
function maybe_int(int $x): ?int {
    if ($x) {
        return 7;
    }
    return null;
}
$a = [maybe_int(1), maybe_int(0)];
var_dump($a[0]);
var_dump($a[1]);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "int(7)\nNULL\n");
}

/// A loop body is lowered ONCE, against the local types that hold at loop ENTRY. An element write
/// whose value type does not fit the array's element type re-types the local to `Array(Mixed)` and
/// emits `Op::ArrayToMixed`, which at runtime REPLACES every element slot with a boxed Mixed cell.
/// Every op lowered ABOVE that write in the same body was therefore compiled against the OLD, raw
/// representation — right on iteration 1, reading a Mixed cell pointer as a raw value from
/// iteration 2 on. It is a back-edge-only bug: straight-line code and `if` branches are correct.
///
/// Here the inner array's push widens `$m` (element `Array(Int)` -> `Array(Mixed)`), so from the
/// second iteration `count($m[0])` read a boxed cell as an array header: `1 5 5 5` instead of
/// `1 2 3 4`. The loop lowerings now pre-widen loop-carried arrays in the preheader and re-lower
/// the body against the widened environment (`stmt::lower_loop_at_type_fixpoint`).
#[test]
fn test_loop_body_nested_push_widening_is_prewidened_in_preheader() {
    let out = compile_and_run_capture(
        r#"<?php $m = [[]]; for ($i = 0; $i < 4; $i++) { $m[0][] = $i; echo count($m[0]), " "; }
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "1 2 3 4 ");
}

/// The same back-edge miscompile through a READ lowered above the widening point: `$b = $m[0]`
/// is compiled against `Array(Array(Int))`, but the `$m[1] = "s"` below it converts `$m`'s slots to
/// boxed Mixed cells. From iteration 2 the read handed `count()` a Mixed cell instead of the inner
/// array, so the count drifted (`3 4 4` instead of `3 3 3`).
#[test]
fn test_loop_body_read_above_widening_point_sees_widened_array() {
    let out = compile_and_run_capture(
        r#"<?php $m = [[1,2,3]]; for ($i = 0; $i < 3; $i++) { $b = $m[0]; echo count($b), " "; $m[1] = "s"; }
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "3 3 3 ");
}

/// The write-side of the same bug, and its worst outcome: `$m[0] = 9` is lowered as a raw int store
/// into an `Array(Int)` slot, but `$m[1] = "s"` below it re-typed `$m` to `Array(Mixed)`. From
/// iteration 2 the int store overwrote a live Mixed cell POINTER with the scalar 9, and the trailing
/// `var_dump($m[0])` dereferenced it — SIGSEGV (exit 139), not merely a wrong value.
#[test]
fn test_loop_body_scalar_write_above_widening_point_does_not_segfault() {
    let out = compile_and_run_capture(
        r#"<?php $m = [1,2,3]; for ($i = 0; $i < 3; $i++) { $m[0] = 9; $m[1] = "s"; } var_dump($m[0]);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "int(9)\n");
}

/// The fixed point must hold for every loop lowering, not just `for`, and the preheader conversion
/// must be idempotent: an inner loop's preheader re-runs on every outer iteration, and
/// `__rt_array_to_mixed` re-stamps an already-Mixed array without re-boxing its slots. The zero-trip
/// case is covered too: code after the loop is compiled against `Array(Mixed)` whether or not the
/// body ever ran, so the array has to be converted even when the loop does not execute.
#[test]
fn test_loop_array_prewidening_covers_while_do_foreach_nested_and_zero_trip() {
    let out = compile_and_run_capture(
        r#"<?php
$m = [1,2,3];
for ($i = 0; $i < 3; $i++) {
    for ($j = 0; $j < 2; $j++) { $m[0] = $i; $m[1] = "s$j"; }
    echo $m[0], "|", $m[1], " ";
}
echo "\n";
$z = [1,2,3];
for ($i = 0; $i < 0; $i++) { $z[1] = "s"; }
var_dump($z[0]);
$w = [1,2,3];
$k = 0;
while ($k < 3) { echo $w[0], " "; $w[0] = "x"; $k++; }
echo "\n";
$d = [1,2,3];
$n = 0;
do { echo $d[1], " "; $d[1] = "y"; $n++; } while ($n < 3);
echo "\n";
$f = [1,2,3];
foreach ([0,1] as $ignored) { echo $f[0], " "; $f[0] = "z"; }
echo "\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "0|s1 1|s1 2|s1 \nint(1)\n1 x x \n2 y y \n1 z \n"
    );
}

// --- `if` arm-exit type join: an element write widens an array on ONE arm only ---
//
// Every array below is built from a PARAMETER on purpose. An array built from a plain literal is
// const-folded — `echo $m[0]` lowers to `const_i64 1` — so a literal-only fixture would pass
// VACUOUSLY without ever exercising the widened representation. A helper returning `array` is the
// other trap: it yields `Array(Mixed)`, which is already boxed and hides the bug too.

/// A bare `if` with NO else, and a read after it. The then-arm's element write re-types `$m` to
/// `Array(Mixed)` — which at runtime replaces every element slot with a boxed cell pointer — and
/// that type fact used to leak straight past the arm split into the implicit else and into the
/// code after the `if` (the tail-sinking pass copies it into every fall-through arm). On the
/// `f(0)` call the array is still raw, so the boxed read dereferenced a raw integer: the second
/// line of output was silently lost.
#[test]
fn test_if_then_arm_array_widening_does_not_leak_into_implicit_else() {
    let out = compile_and_run_capture(
        r#"<?php
function f(int $c): void {
    $m = [$c, 2, 3];
    if ($c > 0) { $m[1] = "s"; }
    echo $m[0], "\n";
}
f(1);
f(0);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "1\n0\n");
}

/// The else arm REBUILDS the array, so no conversion placed in a dominator of the `if` could ever
/// fix it: whatever the preheader (or the block before the `if`) converts, `$m = [...]` overwrites
/// with a fresh, concretely-typed array. The conversion has to live on the arm's own exit edge.
#[test]
fn test_if_else_arm_rebuilt_array_is_converted_on_its_own_exit_edge() {
    let out = compile_and_run_capture(
        r#"<?php
function f(int $c): void {
    $m = [$c, 2, 3];
    if ($c > 0) { $m[1] = "s"; } else { $m = [$c + 7, $c + 8, $c + 9]; }
    echo $m[0], "|", $m[1], "\n";
}
f(1);
f(0);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "1|s\n7|8\n");
}

/// The same defect through an `elseif` chain. Every arm of the chain branches to ONE merge, so the
/// join has to be computed across all of them at once — a pairwise join per recursion level would
/// reconcile the wrong pairs. This used to lose most of the program's output.
#[test]
fn test_if_elseif_chain_joins_every_arm_against_one_merge() {
    let out = compile_and_run_capture(
        r#"<?php
function g(int $c): void {
    $m = [$c, 2, 3];
    if ($c === 1) { $m[1] = "one"; }
    elseif ($c === 2) { $m[1] = "two"; }
    else { $m[1] = "other"; }
    echo $m[0], " ", $m[1], "\n";
}
g(0);
g(1);
g(2);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "0 other\n1 one\n2 two\n");
}

/// A widening `if` inside a loop body needs BOTH halves of the fix: the loop preheader converts the
/// array once, so the read above the `if` (compiled against the widened type on every iteration)
/// sees boxed slots; and the arm join converts the array the else arm REBUILDS, so the back edge
/// does not carry a raw array into a header compiled for a boxed one.
#[test]
fn test_loop_body_if_arm_rebuilds_array_and_back_edge_stays_boxed() {
    let out = compile_and_run_capture(
        r#"<?php
function h(int $c): void {
    $m = [$c, 2, 3];
    for ($i = 0; $i < 3; $i++) {
        echo $m[0], " ";
        if ($i == 0) { $m[1] = "s"; } else { $m = [7, 8, 9]; }
    }
    echo "\n";
    $w = [$c, 2, 3];
    $k = 0;
    while ($k < 3) {
        echo $w[0], " ";
        if ($k == 0) { $w[1] = "s"; } else { $w = [7, 8, 9]; }
        $k++;
    }
    echo "\n";
}
h(1);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "1 1 7 \n1 1 7 \n");
}

/// The widening arm `continue`s, so it is TERMINATED: it contributes no edge to the merge and its
/// type fact is rolled back at the arm split. The loop's pre-widening discovery therefore cannot
/// read the widening off the body's exit environment — it has to read the sticky record every
/// element widening leaves behind (`LoweringContext::widened_indexed_arrays`). Without that record
/// the preheader conversion is skipped while the body is still lowered against the widened array.
#[test]
fn test_loop_widening_arm_that_continues_is_still_pre_widened() {
    let out = compile_and_run_capture(
        r#"<?php
function c(int $n): void {
    $m = [$n, 2, 3];
    for ($i = 0; $i < 4; $i++) {
        if ($i == 1) { $m[1] = "s"; continue; }
        echo $m[0], " ";
    }
    echo "\n";
    echo $m[1], "\n";
}
c(4);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "4 4 4 \ns\n");
}

/// The same, for a widening arm that `break`s out of the loop: the array is read after the loop,
/// where it is typed `Array(Mixed)`, so it must actually have been converted on that path.
#[test]
fn test_loop_widening_arm_that_breaks_is_still_pre_widened() {
    let out = compile_and_run_capture(
        r#"<?php
function b(int $n): void {
    $m = [$n, 2, 3];
    for ($i = 0; $i < 4; $i++) {
        if ($i == 2) { $m[1] = "s"; break; }
        echo $m[0], " ";
    }
    echo "\n";
    echo $m[0], " ", $m[1], "\n";
}
b(4);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "4 4 \n4 s\n");
}

/// Both arms start from an EMPTY literal, whose element type is the `Never`/`Void` placeholder, and
/// each appends a different concrete type. The join must not let either arm silently adopt the
/// other's element type: it widens to `Array(Mixed)` and converts both arms. Converting a
/// zero-length array is O(1), so paying for it is cheaper than depending on what an empty literal
/// stamps into the array header.
#[test]
fn test_if_arms_appending_different_types_to_an_empty_array_join_to_mixed() {
    let out = compile_and_run_capture(
        r#"<?php
function e(int $c): void {
    $m = [];
    if ($c > 0) { $m[] = $c; } else { $m[] = "s"; }
    echo $m[0], "\n";
}
e(1);
e(0);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "1\ns\n");
}

/// KNOWN STILL BROKEN — `switch` has the same defect and the `if` arm-tail join does NOT fix it.
/// Every case body is lowered against one shared, forward-leaking environment, and PHP fall-through
/// gives each case TWO predecessors (its own `case` edge and the previous body's fall-through), so
/// the reconciliation a `switch` needs is a join at each case HEAD, not one at a single merge.
/// That is strictly bigger than the `if` fix and is deliberately left out of it.
#[test]
#[ignore = "switch case bodies share one env and each case head has two predecessors; needs a per-case-head join"]
fn test_switch_case_array_widening_does_not_leak_into_later_cases() {
    let out = compile_and_run_capture(
        r#"<?php
function s(int $c): void {
    $m = [$c, 2, 3];
    switch ($c) {
        case 1: $m[1] = "s"; break;
        default: break;
    }
    echo $m[0], "\n";
}
s(1);
s(0);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "1\n0\n");
}

/// KNOWN STILL BROKEN — `try`/`catch` has the same defect and an exit-edge join cannot fix it. The
/// handler is lowered against the try body's EXIT environment, but a handler is reachable from
/// EVERY point inside the try, including from above the widening write. Making it correct requires
/// pre-widening at the try's ENTRY (the same trick the loop preheader uses), not a tail join.
#[test]
#[ignore = "catch is lowered against the try body's exit env but is reachable from every point in the try; needs try-entry pre-widening"]
fn test_try_body_array_widening_does_not_leak_into_the_catch_handler() {
    let out = compile_and_run_capture(
        r#"<?php
function t(int $c): void {
    $m = [$c, 2, 3];
    try {
        if ($c > 0) { throw new Exception("x"); }
        $m[1] = "s";
    } catch (Exception $e) {
        echo $m[0], "\n";
    }
    echo $m[0], "\n";
}
t(1);
t(0);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "1\n1\n0\n");
}
