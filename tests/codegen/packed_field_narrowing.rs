//! Purpose:
//! Regression tests for values whose type is only known at run time crossing packed class
//! fields: boxed values stored into `float`, `f32`, `bool` and integer fields, and scalar field
//! reads feeding array literals and branch merges.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Float arithmetic over a loop counter (`1.5 * ($i + 1)`) is typed `float` by the checker
//!   but stays a boxed `Mixed` in EIR; the store used to write the box ADDRESS into the field.
//! - Each scalar field narrows a boxed value strictly: its own type passes, an int widens into a
//!   float field, and anything else throws a catchable `TypeError` before the field changes.
//! - The loop fixtures run with the EIR optimizer on and off, and the heap-debug fixtures prove
//!   the narrowing releases every box it consumes, on the throwing path as well.

use crate::support::*;

/// The original report: float arithmetic over a counted loop index stored into a packed float
/// field through a buffer element.
const COUNTED_LOOP_FLOAT_STORE: &str = r#"<?php
packed class Vec2 { public float $x; public float $y; }
$pts = buffer_new<Vec2>(3);
for ($i = 0; $i < 3; $i++) { $pts[$i]->x = 1.5 * ($i + 1); }
echo $pts[0]->x, "|", $pts[1]->x, "|", $pts[2]->x, "\n";
"#;

/// Verifies a float computed from a loop counter is stored as its value, not as the address of
/// the box carrying it (`2.8587270672401E-317` before the fix), with the optimizer on.
#[test]
fn test_packed_float_field_stores_counted_loop_float_arithmetic() {
    assert_eq!(compile_and_run(COUNTED_LOOP_FLOAT_STORE), "1.5|3|4.5\n");
}

/// Verifies the same store without the EIR optimizer, where every intermediate stays boxed.
#[test]
fn test_packed_float_field_stores_counted_loop_float_arithmetic_without_ir_opt() {
    without_ir_opt(|| {
        assert_eq!(compile_and_run(COUNTED_LOOP_FLOAT_STORE), "1.5|3|4.5\n");
    });
}

/// Verifies every packed store path narrows the boxed value: a `ptr<T>` receiver, an `f32`
/// field, a nested record field reached through a buffer element, and compound assignments.
#[test]
fn test_packed_float_fields_narrow_on_every_store_path() {
    let out = compile_and_run(
        r#"<?php
extern function calloc(u64 $count, u64 $size): ptr;
packed class Vec2 { public float $x; public f32 $y; }
packed class Body { public Vec2 $pos; public int $id; }
$p = ptr_cast<Vec2>(calloc(1, ptr_sizeof("Vec2")));
$bodies = buffer_new<Body>(3);
for ($i = 0; $i < 3; $i++) {
    $p->x = 1.5 * ($i + 1);
    $p->y = 0.25 * ($i + 1);
    $bodies[$i]->pos->x = 2.5 * ($i + 1);
    $bodies[$i]->pos->y += 0.5 * ($i + 1);
    $bodies[$i]->pos->y += 0.5 * ($i + 1);
}
echo $p->x, " ", $p->y, " ", $bodies[0]->pos->x, " ", $bodies[2]->pos->x, " ", $bodies[2]->pos->y, "\n";
"#,
    );
    assert_eq!(out, "4.5 0.75 2.5 7.5 3\n");
}

/// Verifies a float field takes a runtime float as it is and a runtime int converted to the
/// nearest float, whether the value comes from a `mixed` parameter, `json_decode()`, a declared
/// `int|float` parameter or an untyped function.
#[test]
fn test_packed_float_field_converts_runtime_ints_and_keeps_runtime_floats() {
    let out = compile_and_run(
        r#"<?php
packed class Vec2 { public float $x; public f32 $y; }
function load(mixed $v) { return $v; }
function scaled($a) { return $a * 1.5; }
function put(buffer<Vec2> $b, int|float $v): void { $b[0]->x = $v; }
$pts = buffer_new<Vec2>(1);
$pts[0]->x = load(2.5);
var_dump($pts[0]->x);
$pts[0]->x = load(3);
var_dump($pts[0]->x);
$pts[0]->x = json_decode("-4");
var_dump($pts[0]->x);
$pts[0]->y = json_decode("0.5");
var_dump($pts[0]->y);
$pts[0]->y = load(7);
var_dump($pts[0]->y);
put($pts, 2);
var_dump($pts[0]->x);
put($pts, 6.25);
var_dump($pts[0]->x);
$pts[0]->x = scaled($argc);
var_dump($pts[0]->x);
"#,
    );
    assert_eq!(
        out,
        "float(2.5)\nfloat(3)\nfloat(-4)\nfloat(0.5)\nfloat(7)\nfloat(2)\nfloat(6.25)\nfloat(1.5)\n"
    );
}

/// Verifies a float field refuses every runtime value that is not a number with a catchable
/// `TypeError` naming the runtime type, instead of a PHP `(float)` cast, and keeps its value.
#[test]
fn test_packed_float_field_rejects_runtime_non_numbers() {
    let out = compile_and_run(
        r#"<?php
packed class Vec2 { public float $x; public float $y; }
$pts = buffer_new<Vec2>(1);
$pts[0]->x = 1.25;
foreach (["\"2.5\"", "null", "[1]", "true", "{\"a\":1}"] as $json) {
    try {
        $pts[0]->x = json_decode($json);
        echo "stored\n";
    } catch (TypeError $e) {
        echo $e->getMessage(), "\n";
    }
}
var_dump($pts[0]->x);
"#,
    );
    assert_eq!(
        out,
        "Packed field Vec2::$x must be of type float, string given\n\
         Packed field Vec2::$x must be of type float, null given\n\
         Packed field Vec2::$x must be of type float, array given\n\
         Packed field Vec2::$x must be of type float, bool given\n\
         Packed field Vec2::$x must be of type float, object given\n\
         float(1.25)\n"
    );
}

/// Verifies a float expression over a bare loop counter, which the checker types `mixed`, is
/// accepted into a float field instead of failing to compile.
#[test]
fn test_packed_float_field_accepts_float_plus_loop_counter() {
    let out = compile_and_run(
        r#"<?php
packed class P { public float $x; }
$b = buffer_new<P>(6);
$n = 0;
for ($i = 0; $i < 2; $i++) {
    for ($k = 0; $k < 3; $k++) {
        $b[$n]->x = $i * 10.0 + $k;
        $n++;
    }
}
echo $b[0]->x, " ", $b[4]->x, " ", $b[5]->x, "\n";
"#,
    );
    assert_eq!(out, "0 11 12\n");
}

/// Verifies a bool field takes a runtime bool and refuses any other runtime value with a
/// `TypeError`, where a truthiness cast would have turned the value into a flag.
#[test]
fn test_packed_bool_field_narrows_runtime_values_strictly() {
    let out = compile_and_run(
        r#"<?php
packed class Cell { public bool $live; public int $n; }
function load(mixed $v) { return $v; }
$cells = buffer_new<Cell>(1);
$cells[0]->live = json_decode("true");
var_dump($cells[0]->live);
$cells[0]->live = load(false);
var_dump($cells[0]->live);
try {
    $cells[0]->live = json_decode("1");
} catch (TypeError $e) {
    echo $e->getMessage(), "\n";
}
var_dump($cells[0]->live);
"#,
    );
    assert_eq!(
        out,
        "bool(true)\nbool(false)\nPacked field Cell::$live must be of type bool, int given\nbool(false)\n"
    );
}

/// Verifies the narrowing releases every box it consumes: a loop storing boxed floats and
/// bools into packed fields leaves a clean heap, with the optimizer on and off.
#[test]
fn test_packed_field_narrowing_loop_is_heap_clean() {
    let source = r#"<?php
packed class Vec2 { public float $x; public bool $live; }
function load(mixed $v) { return $v; }
$pts = buffer_new<Vec2>(4);
$sum = 0.0;
for ($i = 0; $i < 2000; $i++) {
    $pts[$i % 4]->x = 1.5 * ($i + 1);
    $pts[$i % 4]->live = load($i % 2 == 0);
    $sum += $pts[$i % 4]->x;
}
$pts[0]->x = load(3);
echo $sum, " ", $pts[0]->x, " ", $pts[3]->live ? "on" : "off", "\n";
buffer_free($pts);
"#;
    for optimized in [true, false] {
        let out = if optimized {
            compile_and_run_with_heap_debug(source)
        } else {
            without_ir_opt(|| compile_and_run_with_heap_debug(source))
        };
        assert!(out.success, "optimized={optimized}: {}", out.stderr);
        assert_eq!(out.stdout, "3001500 3 off\n", "optimized={optimized}");
        assert!(
            out.stderr.contains("HEAP DEBUG: leak summary: clean"),
            "optimized={optimized}: expected a clean heap, got: {}",
            out.stderr
        );
    }
}

/// Verifies a refused temporary is still released when the narrowing throws: the unwind retires
/// the box before the catch body, so an object it carries is destroyed there, as PHP frees a
/// live temporary while unwinding, and repeated refusals leave a clean heap.
#[test]
fn test_packed_field_narrowing_releases_refused_temporaries() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
packed class Cell { public int $id; public float $x; public bool $live; }
class Noisy { public function __destruct() { echo "freed\n"; } }
function load(mixed $v) { return $v; }
$cells = buffer_new<Cell>(1);
for ($i = 0; $i < 3; $i++) {
    try { $cells[0]->x = json_decode("\"x\""); } catch (TypeError $e) { echo "float refused\n"; }
}
try { $cells[0]->id = load(new Noisy()); } catch (TypeError $e) { echo "int refused\n"; }
try { $cells[0]->live = load([1, 2]); } catch (TypeError $e) { echo "bool refused\n"; }
echo "done\n";
buffer_free($cells);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "float refused\nfloat refused\nfloat refused\nfreed\nint refused\nbool refused\ndone\n"
    );
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected refused temporaries to be released, got: {}",
        out.stderr
    );
}

/// Verifies scalar packed field reads keep their type in array literals and branch merges:
/// the lowering typed every such read `int`, which truncated a float in an indexed literal,
/// stored its raw bits under the int tag in an associative one, and read a bool as `1`.
#[test]
fn test_packed_field_reads_keep_their_type_in_literals_and_merges() {
    let out = compile_and_run(
        r#"<?php
extern function calloc(u64 $count, u64 $size): ptr;
packed class Vec2 { public float $x; public f32 $y; public bool $live; public u8 $tag; }
$pts = buffer_new<Vec2>(1);
$pts[0]->x = 2.5;
$pts[0]->y = 0.25;
$pts[0]->live = true;
$pts[0]->tag = 7;
$q = ptr_cast<Vec2>(calloc(1, ptr_sizeof("Vec2")));
$q->x = -1.5;
var_dump([$pts[0]->x, $pts[0]->y, $pts[0]->live, $pts[0]->tag]);
var_dump(["x" => $q->x, "live" => $pts[0]->live]);
$merged = $argc > 5 ? 9 : $pts[0]->x;
var_dump($merged);
var_dump([$q->x]);
"#,
    );
    assert_eq!(
        out,
        "array(4) {\n  [0]=>\n  float(2.5)\n  [1]=>\n  float(0.25)\n  [2]=>\n  bool(true)\n  [3]=>\n  int(7)\n}\n\
         array(2) {\n  [\"x\"]=>\n  float(-1.5)\n  [\"live\"]=>\n  bool(true)\n}\n\
         float(2.5)\n\
         array(1) {\n  [0]=>\n  float(-1.5)\n}\n"
    );
}

/// Verifies a ternary or `match` whose arms are literals of packed field reads keeps the reads'
/// types: each arm now builds its literal with float and bool elements, and the merge must type
/// the arm the same way instead of the syntactic `int` it read the float's bits back with.
#[test]
fn test_packed_field_reads_keep_their_type_in_literal_merge_arms() {
    let out = compile_and_run(
        r#"<?php
packed class P { public float $x; public bool $live; }
$b = buffer_new<P>(1);
$b[0]->x = 2.5;
$b[0]->live = true;
$r = $argc > 0 ? [$b[0]->x] : [0];
var_dump($r[0]);
$m = match ($argc) { 1 => [$b[0]->x, $b[0]->live], default => [] };
var_dump($m[1]);
"#,
    );
    assert_eq!(out, "float(2.5)\nbool(true)\n");
}

/// Verifies packed float and bool fields stored from buffer merges: an element of a ternary or
/// `match` of two buffers is a plain scalar, and float arithmetic over it with a `mixed` value is
/// narrowed into the field like any other runtime value.
#[test]
fn test_packed_fields_store_buffer_merge_elements() {
    let out = compile_and_run(
        r#"<?php
packed class Cell { public float $x; public bool $live; }
function load(mixed $v) { return $v; }
$fa = buffer_new<float>(1);
$fb = buffer_new<float>(1);
$fa[0] = 2.5;
$fb[0] = 4.5;
$flags = buffer_new<bool>(1);
$flags[0] = true;
$cells = buffer_new<Cell>(1);
$cells[0]->x = ($argc > 0 ? $fa : $fb)[0];
$picked = match ($argc) { 1 => $fb, default => $fa };
$cells[0]->x += $picked[0] * load(2);
$cells[0]->live = ($argc > 0 ? $flags : $flags)[0];
echo $cells[0]->x, "\n";
var_dump($cells[0]->live);
"#,
    );
    assert_eq!(out, "11.5\nbool(true)\n");
}

/// Verifies stores through a local that starts as `null` and later holds a buffer element: the
/// local is one pointer word, a `ptr<T>` receiver once narrowed, and boxed values written through
/// it are narrowed into the record's float and bool fields.
#[test]
fn test_packed_fields_narrow_through_a_null_then_record_local() {
    let out = compile_and_run(
        r#"<?php
packed class Body { public float $x; public bool $live; }
function load(mixed $v) { return $v; }
$bodies = buffer_new<Body>(3);
$hit = null;
for ($i = 0; $i < 3; $i++) {
    if ($i == 1) { $hit = $bodies[$i]; }
}
if ($hit !== null) {
    $hit->x = 1.5 * ($argc + 1);
    $hit->live = load(true);
}
echo $bodies[1]->x, " ", $bodies[0]->x, "\n";
var_dump($bodies[1]->live);
"#,
    );
    assert_eq!(out, "3 0\nbool(true)\n");
}
