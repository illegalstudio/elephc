//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of indexed array array slicing, stack, and range builtins, including slice, shift, and shift empty.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

/// Tests `array_slice($a, $offset, $length)` with a 5-element array, offset 1, length 3.
/// Verifies correct sub-sequence extraction (20 30 40) and that indices map correctly.
#[test]
fn test_array_slice() {
    let out = compile_and_run(
        r#"<?php
$a = [10, 20, 30, 40, 50];
$b = array_slice($a, 1, 3);
echo $b[0] . " " . $b[1] . " " . $b[2];
"#,
    );
    assert_eq!(out, "20 30 40");
}

/// Tests `array_shift` removes and returns the first element from a 3-element array.
/// Verifies the popped value (10) and that remaining array length is reduced to 2.
#[test]
fn test_array_shift() {
    let out = compile_and_run(
        r#"<?php
$a = [10, 20, 30];
$first = array_shift($a);
echo $first . " " . count($a);
"#,
    );
    assert_eq!(out, "10 2");
}

/// Tests `array_shift` on a single-element array, then on an already-empty array.
/// Verifies that calling shift on empty returns empty string (no output).
#[test]
fn test_array_shift_empty() {
    let out = compile_and_run("<?php $a = [1]; array_shift($a); echo array_shift($a);");
    assert_eq!(out, "");
}

/// Tests `array_unshift` prepends a value to an array and returns the new count.
/// Verifies new length is returned (3) and that the prepended element is at index 0.
#[test]
fn test_array_unshift() {
    let out = compile_and_run(
        r#"<?php
$a = [2, 3];
$n = array_unshift($a, 1);
echo $n . " " . $a[0];
"#,
    );
    assert_eq!(out, "3 1");
}

/// Tests `range($start, $end)` with ascending values (1 to 5).
/// Verifies correct count (5) and iteration order (12345).
#[test]
fn test_range() {
    let out = compile_and_run(
        r#"<?php
$a = range(1, 5);
echo count($a) . ":";
foreach ($a as $v) { echo $v; }
"#,
    );
    assert_eq!(out, "5:12345");
}

/// Tests `range` with start greater than end (5 down to 1), verifying descending order.
/// Verifies correct count (5) and iteration order (54321).
#[test]
fn test_range_descending() {
    let out = compile_and_run(
        r#"<?php
$a = range(5, 1);
echo count($a) . ":";
foreach ($a as $v) { echo $v; }
"#,
    );
    assert_eq!(out, "5:54321");
}

/// Tests `range($start, $end)` when start equals end (3 to 3).
/// Verifies a single-element array is produced with count 1 and value 3.
#[test]
fn test_range_single_element() {
    let out = compile_and_run(
        r#"<?php
$a = range(3, 3);
echo count($a) . ":";
foreach ($a as $v) { echo $v; }
"#,
    );
    assert_eq!(out, "1:3");
}

/// Regression: `range()` and `array_slice()` must unbox a `Mixed`/`Union` integer argument
/// (range end, slice offset) instead of using the boxed heap pointer as a raw int. The int args
/// here are read from a heterogeneous (Mixed-valued) associative array. Before the fix these
/// produced empty results or "heap memory exhausted" (a pointer used as a count).
#[test]
fn test_range_and_slice_unbox_mixed_int_args() {
    let out = compile_and_run(
        r#"<?php
$m = ["n" => 2, "t" => "x"];
echo implode(",", range(1, $m["n"])), "|", implode(",", array_slice([10, 20, 30, 40], $m["n"]));
"#,
    );
    assert_eq!(out, "1,2|30,40");
}

/// Regression: the shared slice/splice/range argument marshaling must unbox a `Mixed` length and a
/// `Mixed` offset on `array_splice` (which mutates its source), and unbox both endpoints of
/// `range()`. The integers are read from a heterogeneous (`Mixed`-valued) associative array, so the
/// boxed-pointer-as-int bug would corrupt the offset, length, removed slice, and remaining array.
#[test]
fn test_slice_splice_range_unbox_mixed_offset_and_length() {
    let out = compile_and_run(
        r#"<?php
$m = ["off" => 1, "len" => 2, "t" => "x"];
echo implode(",", array_slice([10, 20, 30, 40, 50], $m["off"], $m["len"])), "|";
$a = [1, 2, 3, 4, 5];
$removed = array_splice($a, $m["off"], $m["len"]);
echo implode(",", $removed), "|", implode(",", $a), "|";
echo implode(",", range($m["off"], $m["len"]));
"#,
    );
    assert_eq!(out, "20,30|2,3|1,4,5|1,2");
}

/// Regression: when the *array itself* is a boxed `Mixed` cell (read from a heterogeneous associative
/// array), `array_slice`/`array_splice` must still unbox a `Mixed` offset and a `Mixed` length instead
/// of passing the boxed heap pointer as a raw integer. A `Mixed` length previously hard-errored at
/// codegen ("array_slice length PHP type Mixed") and a `Mixed` offset silently corrupted the result.
/// Covers offset+length both Mixed (slice), offset Mixed with length absent (slice), and offset+length
/// both Mixed with source mutation (splice).
#[test]
fn test_mixed_array_slice_splice_unbox_mixed_offset_and_length() {
    let out = compile_and_run(
        r#"<?php
$d = ["arr" => [10, 20, 30, 40, 50], "off" => 1, "len" => 2];
$a = $d["arr"];
echo implode(",", array_slice($a, $d["off"], $d["len"])), "|";
$d2 = ["arr" => [10, 20, 30, 40, 50], "off" => 2];
$a2 = $d2["arr"];
echo implode(",", array_slice($a2, $d2["off"])), "|";
$d3 = ["arr" => [1, 2, 3, 4, 5], "off" => 1, "len" => 2];
$a3 = $d3["arr"];
$removed = array_splice($a3, $d3["off"], $d3["len"]);
echo implode(",", $removed), "|", implode(",", $a3);
"#,
    );
    assert_eq!(out, "20,30|30,40,50|2,3|1,4,5");
}

/// Associative slicing preserves string keys and optionally preserves numeric keys.
#[test]
fn test_array_slice_associative_compatibility_helper() {
    let out = compile_and_run(
        r#"<?php
$source = ['a' => 10, 5 => 20, 'b' => 30, 8 => 40];
$preserved = array_slice($source, 1, 2, true);
foreach ($preserved as $key => $value) { echo $key, '=', $value, ';'; }
echo '|';
$renumbered = array_slice($source, -3, -1, false);
foreach ($renumbered as $key => $value) { echo $key, '=', $value, ';'; }
"#,
    );
    assert_eq!(out, "5=20;b=30;|0=20;b=30;");
}

/// Verifies compatibility routing reads precise static-property metadata before lowering an
/// associative slice with key preservation.
#[test]
fn test_array_slice_associative_static_property() {
    let out = compile_and_run(
        r#"<?php
class SliceCache {
    public static array $buffer = ['a' => 1, 'b' => 2];

    public static function trim(): void {
        self::$buffer = array_slice(self::$buffer, 1, null, true);
    }
}
SliceCache::trim();
echo SliceCache::$buffer['b'];
"#,
    );
    assert_eq!(out, "2");
}

/// Verifies an array-typed helper result is promoted before replacing associative instance
/// property storage.
#[test]
fn test_array_slice_associative_instance_property() {
    let out = compile_and_run(
        r#"<?php
class SliceBuffer {
    public array $values = ['a' => 1, 'b' => 2];

    public function trim(): void {
        $this->values = array_slice($this->values, 1, null, true);
    }
}
$buffer = new SliceBuffer();
$buffer->trim();
echo $buffer->values['b'];
"#,
    );
    assert_eq!(out, "2");
}

/// Verifies a nullable integer slice length is handled after a non-null branch narrows it.
#[test]
fn test_array_slice_nullable_integer_length() {
    let output = compile_and_run(
        r#"<?php
function first_parts(string $value, ?int $limit): string {
    $parts = explode(':', $value, -1);
    return implode(':', $limit === null ? $parts : array_slice($parts, 0, $limit));
}
echo first_parts('a:b:c', 2);
"#,
    );
    assert_eq!(output, "a:b");
}

/// `array_shift()` accepts associative storage, returns the insertion-order head, removes it from
/// the caller-visible array, and retains surviving string keys while renumbering integer keys.
#[test]
fn test_array_shift_associative_mixed_array() {
    let out = compile_and_run(
        r#"<?php
$data = ["prefix" => "root", 7 => 20, "tail" => 30];
$first = array_shift($data);
echo $first, "|", count($data), "|", $data[0], "|", $data["tail"];
"#,
    );
    assert_eq!(out, "root|2|20|30");
}

/// Verifies a generic `array<mixed>` reference parameter accepts an associative-array value,
/// boxes that hash into its Mixed slot, and preserves balanced ownership after the mutation.
#[test]
fn test_array_unshift_assoc_value_into_generic_array_parameter() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function trace_unshift(array &$trace): int {
    return array_unshift($trace, [
        "function" => "new Demo",
        "file" => "demo.php",
        "line" => 319,
    ]);
}
$trace = [["function" => "old", "file" => "old.php", "line" => 1]];
echo trace_unshift($trace), "|", $trace[0]["function"], "|", $trace[0]["line"];
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "2|new Demo|319");
}

/// Verifies an empty local array widens before a gradual value is prepended by reference.
#[test]
fn test_array_unshift_mixed_value_into_empty_local() {
    let out = compile_and_run(
        r#"<?php
function prepend(mixed $value): string {
    $values = [];
    $count = array_unshift($values, $value);
    return $count . ":" . $values[0];
}
echo prepend("ready");
"#,
    );
    assert_eq!(out, "1:ready");
}

/// A boxed gradual operand that contains an associative array routes through the
/// representation-neutral helper and preserves string keys when requested.
#[test]
fn test_array_slice_gradual_associative_operand() {
    let out = compile_and_run(
        r#"<?php
function gradualSliceSource(mixed $value): mixed { return $value; }
$source = gradualSliceSource(["a" => 10, "b" => 20, "c" => 30]);
$slice = array_slice($source, 1, null, true);
echo count($slice), "|", $slice["b"], "|", $slice["c"];
"#,
    );
    assert_eq!(out, "2|20|30");
}
