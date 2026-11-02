//! Purpose:
//! End-to-end tests for C storage widths in packed classes and `buffer<T>` elements: `i8`..`u32`
//! and `f32` fields laid out like a C struct, width-declared scalar buffers, their extension,
//! truncation and rounding rules, nested records of odd sizes, class members, namespaces, and
//! hoisted loop accesses.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - A C fixture assembled into the program compares the packed layout against the C compiler's
//!   `sizeof` and reads the fields PHP wrote through `ptr<T>`.
//! - The loop fixture runs with the EIR optimizer on and off, so the hoisted
//!   `buffer_get_resolved` and `buffer_set_resolved` forms keep the element width as well as
//!   the plain forms.

use crate::support::*;

/// C structs the packed classes in the fixture test mirror field for field.
const WIDTH_FIXTURE: &str = r#"
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>

typedef struct { float x, y; uint8_t r, g, b, a; float u, v; } elephc_test_vertex;
typedef struct { int32_t id; float x0, y0, x1, y1; int32_t cx0, cy0, cx1, cy1; } elephc_test_gridbox;

static elephc_test_vertex vertex;
static elephc_test_gridbox box;
static char report[256];

size_t elephc_test_width_sizeof(int32_t which) {
    return which == 0 ? sizeof(elephc_test_vertex) : sizeof(elephc_test_gridbox);
}

void *elephc_test_vertex_filled(void) {
    vertex.x = 1.5f;
    vertex.y = -2.25f;
    vertex.r = 255;
    vertex.g = 128;
    vertex.b = 7;
    vertex.a = 200;
    vertex.u = 0.5f;
    vertex.v = 0.75f;
    return &vertex;
}

void *elephc_test_gridbox_blank(void) { memset(&box, 0, sizeof box); return &box; }

const char *elephc_test_describe_gridbox(void) {
    snprintf(report, sizeof report, "%d %.2f %.2f %.2f %.2f %d %d %d %d",
        box.id, (double)box.x0, (double)box.y0, (double)box.x1, (double)box.y1,
        box.cx0, box.cy0, box.cx1, box.cy1);
    return report;
}
"#;

/// Loop fixture over width-declared scalar buffers; the sums wrap exactly as the widths do.
const WIDTH_LOOP_PROGRAM: &str = r#"<?php
$bytes = buffer_new<u8>(300);
for ($i = 0; $i < buffer_len($bytes); $i++) { $bytes[$i] = $i * 3; }
$sum = 0;
for ($i = 0; $i < buffer_len($bytes); $i++) { $sum += $bytes[$i]; }
$shorts = buffer_new<u16>(20000);
for ($i = 0; $i < buffer_len($shorts); $i++) { $shorts[$i] = $i * 7; }
$shortSum = 0;
for ($i = 0; $i < buffer_len($shorts); $i++) { $shortSum += $shorts[$i]; }
$ints = buffer_new<i32>(2000);
for ($i = 0; $i < buffer_len($ints); $i++) { $ints[$i] = $i * 3000000; }
$intSum = 0;
for ($i = 0; $i < buffer_len($ints); $i++) { $intSum += $ints[$i]; }
$floats = buffer_new<f32>(1000);
for ($i = 0; $i < buffer_len($floats); $i++) { $floats[$i] = $i * 0.5; }
$floatSum = 0.0;
for ($i = 0; $i < buffer_len($floats); $i++) { $floatSum += $floats[$i]; }
$sbytes = buffer_new<i8>(300);
for ($i = 0; $i < buffer_len($sbytes); $i++) { $sbytes[$i] = $i * 3; }
$sbyteSum = 0;
for ($i = 0; $i < buffer_len($sbytes); $i++) { $sbyteSum += $sbytes[$i]; }
$sshorts = buffer_new<i16>(20000);
for ($i = 0; $i < buffer_len($sshorts); $i++) { $sshorts[$i] = $i * 7; }
$sshortSum = 0;
for ($i = 0; $i < buffer_len($sshorts); $i++) { $sshortSum += $sshorts[$i]; }
$uints = buffer_new<u32>(2000);
for ($i = 0; $i < buffer_len($uints); $i++) { $uints[$i] = $i * 3000000; }
$uintSum = 0;
for ($i = 0; $i < buffer_len($uints); $i++) { $uintSum += $uints[$i]; }
packed class GridBox { public i32 $id; public f32 $x0; public f32 $y0; public f32 $x1; public f32 $y1; public u8 $flags; }
$boxes = buffer_new<GridBox>(500);
for ($i = 0; $i < buffer_len($boxes); $i++) {
    $boxes[$i]->id = $i;
    $boxes[$i]->x0 = $i * 0.25;
    $boxes[$i]->x1 = $boxes[$i]->x0 + 1.5;
    $boxes[$i]->flags = $i;
}
$hits = 0;
$width = 0.0;
$flagSum = 0;
for ($i = 0; $i < buffer_len($boxes); $i++) {
    if ($boxes[$i]->x1 > 100.0) { $hits++; }
    $width += $boxes[$i]->x1 - $boxes[$i]->x0;
    $flagSum += $boxes[$i]->flags;
}
echo $sum, " ", $shortSum, " ", $intSum, " ", $floatSum, "\n";
echo $sbyteSum, " ", $sshortSum, " ", $uintSum, "\n";
echo ptr_sizeof("GridBox"), " ", $hits, " ", $width, " ", $flagSum, "\n";
"#;

/// Output of `WIDTH_LOOP_PROGRAM`: every sum wraps at the element width (signed widths
/// sign-extend, unsigned ones zero-extend), the record is 24 bytes.
const WIDTH_LOOP_OUTPUT: &str =
    "35478 619265168 482261991936 249750\n2454 5717136 3557458575872\n24 105 750 62286\n";

/// Verifies width-declared packed fields take their natural C size and alignment: the record is
/// padded to its widest field, a class of 8-byte fields keeps its former sequential layout, and
/// `i64`, `u64`, `f64` are the plain word types.
#[test]
fn test_packed_width_fields_have_the_natural_c_layout() {
    let out = compile_and_run(
        r#"<?php
packed class Vertex { public f32 $x; public f32 $y; public u8 $r; public u8 $g; public u8 $b; public u8 $a; public f32 $u; public f32 $v; }
packed class GridBox { public i32 $id; public f32 $x0; public f32 $y0; public f32 $x1; public f32 $y1; public i32 $cx0; public i32 $cy0; public i32 $cx1; public i32 $cy1; }
packed class Padded { public u8 $a; public i32 $b; public u16 $c; public int $d; }
packed class Legacy { public int $x; public float $y; public bool $z; public ptr $p; }
packed class Tiny { public u8 $flag; }
packed class Words { public i64 $a; public u64 $b; public f64 $c; }
echo ptr_sizeof("Vertex"), " ", ptr_sizeof("GridBox"), " ", ptr_sizeof("Padded"), " ", ptr_sizeof("Legacy"), " ", ptr_sizeof("Tiny"), " ", ptr_sizeof("Words"), "\n";
"#,
    );
    assert_eq!(out, "20 36 24 32 1 24\n");
}

/// Verifies width-declared fields read and write like extern class fields of the same width:
/// signed widths sign-extend, unsigned widths zero-extend, an out-of-range integer keeps its low
/// bits, `f32` rounds to single precision, neighbouring fields and elements stay untouched, and
/// compound assignments, increments and arithmetic results narrow the same way.
#[test]
fn test_packed_width_fields_extend_and_truncate_like_c() {
    let out = compile_and_run(
        r#"<?php
packed class All { public i8 $i8; public u8 $u8; public i16 $i16; public u16 $u16; public i32 $i32; public u32 $u32; public f32 $f32; public int $i64; }
$b = buffer_new<All>(2);
$b[1]->i8 = 200;
$b[1]->u8 = 300;
$b[1]->i16 = 40000;
$b[1]->u16 = -1;
$b[1]->i32 = 5000000000;
$b[1]->u32 = -1;
$b[1]->f32 = 0.1;
$b[1]->i64 = -5;
echo $b[1]->i8, " ", $b[1]->u8, " ", $b[1]->i16, " ", $b[1]->u16, " ", $b[1]->i32, " ", $b[1]->u32, " ", $b[1]->f32, " ", $b[1]->i64, "\n";
echo $b[0]->i8, " ", $b[0]->u8, " ", $b[0]->i32, " ", $b[0]->f32, " ", $b[0]->i64, "\n";
$b[1]->u8 = 255;
$b[1]->u8++;
$b[1]->i32 += 10;
$b[1]->f32 = 1.5;
$b[1]->f32 += 0.25;
$b[1]->i16--;
echo $b[1]->u8, " ", $b[1]->i32, " ", $b[1]->f32, " ", $b[1]->i16, "\n";
for ($i = 0; $i < 3; $i++) {
    $b[0]->u16 = $i * 40000;
    echo $b[0]->u16, " ";
}
echo "\n";
"#,
    );
    assert_eq!(
        out,
        "-56 44 -25536 65535 705032704 4294967295 0.10000000149012 -5\n0 0 0 0 0\n0 705032714 1.75 -25537\n0 40000 14464 \n"
    );
}

/// Verifies nested records of width-declared fields embed at their own alignment and copy as a
/// whole when their size is not a multiple of 8: a 3-byte colour, a 6-byte pixel, a 12-byte
/// integer vector inside a 16-byte cell. The byte right after the 3-byte copy is written before
/// it, so a copy one byte too long would show.
#[test]
fn test_packed_width_nested_records_copy_odd_sizes() {
    let out = compile_and_run(
        r#"<?php
packed class Rgb { public u8 $r; public u8 $g; public u8 $b; }
packed class Pixel { public Rgb $color; public u8 $alpha; public i16 $depth; }
packed class Vec3i { public i32 $x; public i32 $y; public i32 $z; }
packed class Cell { public Vec3i $pos; public u8 $kind; }
echo ptr_sizeof("Rgb"), " ", ptr_sizeof("Pixel"), " ", ptr_sizeof("Vec3i"), " ", ptr_sizeof("Cell"), "\n";
$px = buffer_new<Pixel>(2);
$px[0]->color->r = 10;
$px[0]->color->g = 20;
$px[0]->color->b = 300;
$px[0]->alpha = 40;
$px[0]->depth = -7;
$px[1]->depth = 9;
$px[1]->alpha = 99;
$px[1]->color = $px[0]->color;
echo $px[1]->color->r, " ", $px[1]->color->g, " ", $px[1]->color->b, " ", $px[1]->alpha, " ", $px[1]->depth, "\n";
$cells = buffer_new<Cell>(2);
$cells[0]->pos->x = 1;
$cells[0]->pos->y = -2;
$cells[0]->pos->z = 3000000000;
$cells[0]->kind = 7;
$cells[1]->pos = $cells[0]->pos;
echo $cells[1]->pos->x, " ", $cells[1]->pos->y, " ", $cells[1]->pos->z, " ", $cells[1]->kind, "\n";
"#,
    );
    assert_eq!(out, "3 6 12 16\n10 20 44 99 9\n1 -2 -1294967296 0\n");
}

/// Verifies a packed class of width-declared fields has the C compiler's `sizeof` and offsets:
/// through `ptr<T>`, PHP reads the fields C filled and C reads the fields PHP wrote, each at its
/// declared width (`5000000000` lands in an `int32_t` as `705032704`, `0.1` as `0.1f`).
#[test]
fn test_packed_width_layout_matches_the_c_struct() {
    let out = compile_and_run_with_c_fixture(
        r#"<?php
packed class Vertex { public f32 $x; public f32 $y; public u8 $r; public u8 $g; public u8 $b; public u8 $a; public f32 $u; public f32 $v; }
packed class GridBox { public i32 $id; public f32 $x0; public f32 $y0; public f32 $x1; public f32 $y1; public i32 $cx0; public i32 $cy0; public i32 $cx1; public i32 $cy1; }
extern function elephc_test_width_sizeof(i32 $which): u64;
extern function elephc_test_vertex_filled(): ptr;
extern function elephc_test_gridbox_blank(): ptr;
extern function elephc_test_describe_gridbox(): string;
echo ptr_sizeof("Vertex"), " ", elephc_test_width_sizeof(0), " ", ptr_sizeof("GridBox"), " ", elephc_test_width_sizeof(1), "\n";
$v = ptr_cast<Vertex>(elephc_test_vertex_filled());
echo $v->x, " ", $v->y, " ", $v->r, " ", $v->g, " ", $v->b, " ", $v->a, " ", $v->u, " ", $v->v, "\n";
$g = ptr_cast<GridBox>(elephc_test_gridbox_blank());
$g->id = -1;
$g->x0 = 0.1;
$g->y0 = 2.5;
$g->x1 = 3.0;
$g->cx0 = 5000000000;
$g->cy1 = -2;
echo elephc_test_describe_gridbox(), "\n";
"#,
        WIDTH_FIXTURE,
    );
    assert_eq!(
        out,
        "20 20 36 36\n1.5 -2.25 255 128 7 200 0.5 0.75\n-1 0.10 2.50 3.00 0.00 705032704 0 0 -2\n"
    );
}

/// Verifies scalar buffers of every width store elements at that width with the same extension,
/// truncation and rounding rules, outside loops (the unhoisted `buffer_get` and `buffer_set`),
/// including arithmetic results and compound assignments on an element.
#[test]
fn test_width_buffers_store_elements_at_their_width() {
    let out = compile_and_run(
        r#"<?php
$u8 = buffer_new<u8>(4);
$u8[0] = 300;
$u8[1] = -1;
$u8[2] = 255;
$i8 = buffer_new<i8>(1);
$i8[0] = 200;
$u16 = buffer_new<u16>(1);
$u16[0] = 70000;
$i16 = buffer_new<i16>(1);
$i16[0] = 40000;
$i32 = buffer_new<i32>(2);
$i32[0] = 5000000000;
$i32[1] = -1;
$u32 = buffer_new<u32>(1);
$u32[0] = -1;
$f32 = buffer_new<f32>(2);
$f32[0] = 0.1;
$f32[1] = 2.5;
echo $u8[0], " ", $u8[1], " ", $u8[2], " ", $u8[3], " ", buffer_len($u8), "\n";
echo $i8[0], " ", $u16[0], " ", $i16[0], " ", $i32[0], " ", $i32[1], " ", $u32[0], "\n";
echo $f32[0], " ", $f32[1], "\n";
$u8[3] = $u8[0] + $u8[2];
$u8[2] += 10;
$f32[1] *= 3.0;
echo $u8[3], " ", $u8[2], " ", $f32[1], "\n";
"#,
    );
    assert_eq!(
        out,
        "44 255 255 0 4\n-56 4464 -25536 705032704 -1 4294967295\n0.10000000149012 2.5\n43 9 7.5\n"
    );
}

/// Verifies loops over width-declared buffers and packed records keep the element width through
/// the hoisted descriptor forms the EIR optimizer rewrites them into.
#[test]
fn test_width_buffer_loops_keep_the_element_width_when_hoisted() {
    assert_eq!(compile_and_run(WIDTH_LOOP_PROGRAM), WIDTH_LOOP_OUTPUT);
}

/// Verifies the same loops produce the same output with the EIR optimizer off, so the plain and
/// the hoisted access forms agree on the element width.
#[test]
fn test_width_buffer_loops_keep_the_element_width_without_ir_opt() {
    without_ir_opt(|| assert_eq!(compile_and_run(WIDTH_LOOP_PROGRAM), WIDTH_LOOP_OUTPUT));
}

/// Verifies width-declared buffers and packed classes work as class properties, parameters and
/// inside a namespace: element writes through a property narrow to the width, and the width
/// names are never namespaced.
#[test]
fn test_width_buffers_in_class_members_and_namespaces() {
    let out = compile_and_run(
        r#"<?php
namespace Render;
packed class V { public i32 $x; public u8 $flag; }
final class Mesh {
    public buffer<f32> $pos;
    public buffer<u16> $idx;
    public buffer<V> $verts;
    public function __construct(int $n) {
        $this->pos = buffer_new<f32>($n);
        $this->idx = buffer_new<u16>($n);
        $this->verts = buffer_new<V>($n);
    }
    public function set(int $i, float $p, int $k): void {
        $this->pos[$i] = $p;
        $this->idx[$i] = $k;
        $this->verts[$i]->x = $k * 70000;
        $this->verts[$i]->flag = $k;
    }
}
function total(buffer<u16> $idx, int $n): int {
    $t = 0;
    for ($i = 0; $i < $n; $i++) { $t += $idx[$i]; }
    return $t;
}
$m = new Mesh(3);
$m->set(0, 0.1, 65535);
$m->set(1, 2.0, 65536);
$m->set(2, 3.5, 300);
echo $m->pos[0], " ", $m->pos[1], " ", $m->idx[0], " ", $m->idx[1], " ", total($m->idx, 3), "\n";
echo $m->verts[1]->x, " ", $m->verts[2]->x, " ", $m->verts[2]->flag, "\n";
"#,
    );
    assert_eq!(out, "0.10000000149012 2 65535 0 65835\n292552704 21000000 44\n");
}

/// Verifies an embedded record of width-declared fields declared AFTER its container is sized
/// first and lands at its own 1-byte alignment inside the container, and that copying it into a
/// nested field at a non-zero offset writes exactly its bytes: the sentinel byte after it keeps
/// its value.
#[test]
fn test_packed_width_nested_record_declared_later_copies_at_its_offset() {
    let out = compile_and_run(
        r#"<?php
packed class Outer { public u8 $tag; public Inner $c; public u8 $after; }
packed class Inner { public u8 $r; public u8 $g; public u8 $b; }
$o = buffer_new<Outer>(2);
$o[0]->tag = 9;
$o[0]->c->r = 1;
$o[0]->c->g = 2;
$o[0]->c->b = 3;
$o[1]->tag = 7;
$o[1]->after = 200;
$o[1]->c = $o[0]->c;
echo ptr_sizeof("Outer"), " ", $o[1]->tag, " ", $o[1]->c->r, $o[1]->c->g, $o[1]->c->b, " ", $o[1]->after, "\n";
"#,
    );
    assert_eq!(out, "5 7 123 200\n");
}

/// Verifies width-declared fields beyond the AArch64 scaled-immediate range (a record over
/// 4 KiB) are addressed from a scratch register through a packed receiver: the `u8` fields past
/// offset 4095 and the `f32` and `i32` between them read back what was stored, and the
/// neighbouring word keeps its value.
#[test]
fn test_packed_width_fields_beyond_the_immediate_offset_range() {
    let words: String = (0..520).map(|index| format!("public int $w{index}; ")).collect();
    let out = compile_and_run(&format!(
        r#"<?php
packed class Big {{ {words}public u8 $tail; public f32 $f; public i32 $n; public u8 $after; }}
$b = buffer_new<Big>(2);
$b[1]->w519 = 77;
$b[1]->tail = 513;
$b[1]->f = 2.5;
$b[1]->n = -3;
$b[1]->after = 9;
echo ptr_sizeof("Big"), " ", $b[1]->tail, " ", $b[1]->f, " ", $b[1]->n, " ", $b[1]->after, " ", $b[1]->w519, " ", $b[0]->after, "\n";
"#
    ));
    assert_eq!(out, "4176 1 2.5 -3 9 77 0\n");
}

/// Verifies the 8-byte spellings `i64`, `u64` and `f64` are the plain word types in fields and
/// buffers (a `u64` of all ones reads as the same bits, `-1`), and that a packed `bool` keeps
/// its 8-byte slot and alignment between two `u8` fields.
#[test]
fn test_packed_word_widths_and_bool_slot() {
    let out = compile_and_run(
        r#"<?php
packed class W { public i64 $a; public u64 $b; public f64 $c; public u8 $d; }
packed class BF { public u8 $a; public bool $f; public u8 $z; }
$w = buffer_new<W>(1);
$w[0]->a = -5;
$w[0]->b = -1;
$w[0]->c = 2.5;
$w[0]->d = 300;
$u64 = buffer_new<u64>(1);
$u64[0] = -1;
$f64 = buffer_new<f64>(1);
$f64[0] = 0.1;
$x = buffer_new<BF>(1);
$x[0]->a = 300;
$x[0]->f = true;
$x[0]->z = 7;
echo ptr_sizeof("W"), " ", $w[0]->a, " ", $w[0]->b, " ", $w[0]->c, " ", $w[0]->d, " ", $u64[0], " ", $f64[0], "\n";
echo ptr_sizeof("BF"), " ", $x[0]->a, " ", $x[0]->z, " ";
var_dump($x[0]->f);
"#,
    );
    assert_eq!(out, "32 -5 -1 2.5 44 -1 0.1\n24 44 7 bool(true)\n");
}

/// Verifies a runtime value that is not an int raises a catchable `TypeError` when stored into
/// a width-declared integer field, like a plain `int` field, instead of writing the boxed cell
/// into the narrow slot.
#[test]
fn test_packed_width_field_rejects_a_runtime_float() {
    let out = compile_and_run(
        r#"<?php
packed class P { public u8 $flag; public i32 $n; }
$b = buffer_new<P>(1);
$b[0]->flag = 1;
$value = json_decode("2.5");
try {
    $b[0]->flag = $value;
} catch (TypeError $e) {
    echo $e->getMessage(), "\n";
}
echo $b[0]->flag, "\n";
"#,
    );
    assert_eq!(out, "Packed field P::$flag must be of type int, float given\n1\n");
}

/// Verifies width-declared buffers flow through every signature position: a return type, a
/// promoted constructor parameter, a generic type argument, a closure parameter and an arrow
/// function capture, each keeping the element width. (A variadic buffer parameter builds an
/// array of buffers, which codegen does not support for any buffer type yet.)
#[test]
fn test_width_buffers_through_signatures_generics_and_closures() {
    let out = compile_and_run(
        r#"<?php
function make(int $n): buffer<u8> { $b = buffer_new<u8>($n); $b[0] = 300; return $b; }
final class Holder { public function __construct(public buffer<f32> $p) {} }
final class Store<T> { public function __construct(public buffer<T> $items) {} }
$m = make(3);
$h = new Holder(buffer_new<f32>(2));
$h->p[1] = 0.1;
$s = new Store<u8>(make(2));
$q = buffer_new<u16>(1);
$q[0] = 70000;
$read = function (buffer<u16> $v): int { return $v[0]; };
$next = fn() => $q[0] + 1;
echo $m[0], " ", $h->p[1], " ", $s->items[0], " ", $read($q), " ", $next(), "\n";
"#,
    );
    assert_eq!(out, "44 0.10000000149012 44 4464 4465\n");
}

/// Verifies a program that really declares a class named like a width keeps PHP's class
/// resolution: inside a namespace the `u8` type hint and `new u8` name the same class.
#[test]
fn test_class_named_like_a_width_still_resolves_in_a_namespace() {
    let out = compile_and_run(
        r#"<?php
namespace App;
final class u8 { public function __construct(public int $v) {} }
function show(u8 $x): int { return $x->v; }
echo show(new u8(5)), "\n";
"#,
    );
    assert_eq!(out, "5\n");
}

/// Verifies a class really named like a width stays a class when it is named explicitly: a
/// fully qualified `\u8`, the bare name where that class is visible, and a `use` import of a
/// namespaced `u8` all give a buffer of that packed record, not a byte buffer.
#[test]
fn test_qualified_or_imported_class_named_like_a_width_is_the_class() {
    let out = compile_and_run(
        r#"<?php
packed class u8 { public int $v; }
$q = buffer_new<\u8>(1);
$q[0]->v = 7;
$bare = buffer_new<u8>(1);
$bare[0]->v = 11;
echo $q[0]->v, " ", $bare[0]->v, " ", ptr_sizeof("u8"), "\n";
"#,
    );
    assert_eq!(out, "7 11 8\n");
    let out = compile_and_run(
        r#"<?php
namespace Lib { packed class u8 { public int $v; } }
namespace App {
    use Lib\u8;
    $b = buffer_new<u8>(2);
    $b[1]->v = 9;
    $w = buffer_new<\Lib\u8>(1);
    $w[0]->v = 3;
    $bytes = buffer_new<u16>(1);
    $bytes[0] = 70000;
    echo $b[1]->v, " ", $w[0]->v, " ", $bytes[0], "\n";
}
"#,
    );
    assert_eq!(out, "9 3 4464\n");
}
