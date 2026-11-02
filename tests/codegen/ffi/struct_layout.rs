//! Purpose:
//! Regression tests for `extern class` C struct layouts read and written through `ptr<Class>`.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - The C fixture declares real structs (an SDL3-style keyboard event, `SDL_Rect`, `SDL_Color`,
//!   `SDL_FRect`, and one struct with every field width) and reports their `sizeof`, so the
//!   tests compare elephc's natural layout against the C compiler's on the test target.
//! - C fills fields that PHP reads by name and checks fields PHP wrote, which pins both the
//!   offsets and the per-width extension, truncation, `f32` rounding and one-byte `bool`.
//! - The fixture is assembled into the test program itself, so no shared library is needed.
//! - Values whose type is only known at run time are narrowed strictly into scalar fields, and
//!   field reads keep their declared type inside array literals and branch merges.

use super::*;

/// C fixture with the structs the extern classes in these tests mirror field for field.
const STRUCT_FIXTURE: &str = r#"
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>

typedef struct {
    uint32_t type;
    uint32_t reserved;
    uint64_t timestamp;
    uint32_t windowID;
    uint32_t which;
    uint32_t scancode;
    uint32_t key;
    uint16_t mod;
    uint16_t raw;
    bool down;
    bool repeat;
} elephc_test_key_event;

typedef struct { int32_t x, y, w, h; } elephc_test_rect;
typedef struct { uint8_t r, g, b, a; } elephc_test_color;
typedef struct { float x, y, w, h; } elephc_test_frect;

typedef struct elephc_test_all {
    int8_t i8;
    uint8_t u8;
    int16_t i16;
    uint16_t u16;
    int32_t i32;
    uint32_t u32;
    int64_t i64;
    uint64_t u64;
    float f32;
    double f64;
    bool flag;
    struct elephc_test_all *self;
    const char *name;
    const char *missing;
} elephc_test_all;

static elephc_test_key_event key_event;
static elephc_test_rect rect;
static elephc_test_color color;
static elephc_test_frect frect;
static elephc_test_all all;
static char report[256];

// sizeof of each fixture struct, in the order the PHP tests ask for them.
size_t elephc_test_struct_size(int32_t which) {
    switch (which) {
    case 0: return sizeof(elephc_test_key_event);
    case 1: return sizeof(elephc_test_rect);
    case 2: return sizeof(elephc_test_color);
    case 3: return sizeof(elephc_test_frect);
    default: return sizeof(elephc_test_all);
    }
}

elephc_test_key_event *elephc_test_key_event_filled(void) {
    key_event.type = 0x300;
    key_event.reserved = 0xdeadbeefu;
    key_event.timestamp = 123456789012345ull;
    key_event.windowID = 7;
    key_event.which = 0xffffffffu;
    key_event.scancode = 4;
    key_event.key = 97;
    key_event.mod = 0x8001;
    key_event.raw = 65535;
    key_event.down = true;
    key_event.repeat = false;
    return &key_event;
}

elephc_test_all *elephc_test_all_filled(void) {
    all.i8 = -5;
    all.u8 = 250;
    all.i16 = -1234;
    all.u16 = 65000;
    all.i32 = -100000;
    all.u32 = 4000000000u;
    all.i64 = -5000000000ll;
    all.u64 = UINT64_MAX;
    all.f32 = 1.5f;
    all.f64 = 2.25;
    all.flag = true;
    all.self = &all;
    all.name = "sdl";
    all.missing = NULL;
    return &all;
}

elephc_test_rect *elephc_test_rect_blank(void) { memset(&rect, 0xff, sizeof rect); return &rect; }
elephc_test_color *elephc_test_color_blank(void) { memset(&color, 0, sizeof color); return &color; }
elephc_test_frect *elephc_test_frect_blank(void) { memset(&frect, 0, sizeof frect); return &frect; }
elephc_test_all *elephc_test_all_blank(void) { memset(&all, 0, sizeof all); return &all; }

// What C sees after PHP wrote the fields, formatted for one comparison.
const char *elephc_test_describe_rect(void) {
    snprintf(report, sizeof report, "%d %d %d %d", rect.x, rect.y, rect.w, rect.h);
    return report;
}

const char *elephc_test_describe_color(void) {
    snprintf(report, sizeof report, "%u %u %u %u", color.r, color.g, color.b, color.a);
    return report;
}

const char *elephc_test_describe_frect(void) {
    snprintf(report, sizeof report, "%s %s %.2f %.2f",
        frect.x == 0.1f ? "single" : "not-single", frect.y == 1e-45f ? "denormal" : "other",
        (double)frect.w, (double)frect.h);
    return report;
}

const char *elephc_test_describe_all(void) {
    snprintf(report, sizeof report, "%d %u %d %u %d %u %lld %llu %s %.3f %d %s",
        all.i8, all.u8, all.i16, all.u16, all.i32, all.u32, (long long)all.i64,
        (unsigned long long)all.u64, all.f32 == 0.75f ? "f32" : "bad-f32", all.f64,
        all.flag ? 1 : 0, all.name ? all.name : "(null)");
    return report;
}
"#;

/// Extern class declarations that mirror `STRUCT_FIXTURE`, plus the fixture's entry points.
const STRUCT_DECLS: &str = r#"<?php
extern class KeyEvent {
    public u32 $type;
    public u32 $reserved;
    public u64 $timestamp;
    public u32 $windowID;
    public u32 $which;
    public u32 $scancode;
    public u32 $key;
    public u16 $mod;
    public u16 $raw;
    public bool $down;
    public bool $repeat;
}
extern class Rect { public i32 $x; public i32 $y; public i32 $w; public i32 $h; }
extern class Color { public u8 $r; public u8 $g; public u8 $b; public u8 $a; }
extern class FRect { public f32 $x; public f32 $y; public f32 $w; public f32 $h; }
extern class All {
    public i8 $i8;
    public u8 $u8;
    public i16 $i16;
    public u16 $u16;
    public i32 $i32;
    public u32 $u32;
    public int $i64;
    public u64 $u64;
    public f32 $f32;
    public float $f64;
    public bool $flag;
    public ptr<All> $self;
    public string $name;
    public string $missing;
}
extern function elephc_test_struct_size(i32 $which): u64;
extern function elephc_test_key_event_filled(): ptr<KeyEvent>;
extern function elephc_test_all_filled(): ptr<All>;
extern function elephc_test_rect_blank(): ptr<Rect>;
extern function elephc_test_color_blank(): ptr<Color>;
extern function elephc_test_frect_blank(): ptr<FRect>;
extern function elephc_test_all_blank(): ptr<All>;
extern function elephc_test_describe_rect(): string;
extern function elephc_test_describe_color(): string;
extern function elephc_test_describe_frect(): string;
extern function elephc_test_describe_all(): string;
"#;

/// Runs `body` after the shared extern declarations against the struct fixture.
fn run_struct_program(body: &str) -> String {
    compile_and_run_with_c_fixture(&format!("{STRUCT_DECLS}{body}"), STRUCT_FIXTURE)
}

/// Verifies `ptr_sizeof()` of every extern class equals the C compiler's `sizeof`: fields sit at their
/// natural alignment and the total is padded to the largest one (the keyboard event is 40 bytes, not
/// eleven 8-byte slots: its two trailing `bool`s take one byte each and two bytes of tail padding).
#[test]
fn test_ffi_extern_class_sizeof_matches_the_c_struct() {
    let out = run_struct_program(
        r#"
echo "KeyEvent ", ptr_sizeof("KeyEvent"), " ", elephc_test_struct_size(0), "\n";
echo "Rect ", ptr_sizeof("Rect"), " ", elephc_test_struct_size(1), "\n";
echo "Color ", ptr_sizeof("Color"), " ", elephc_test_struct_size(2), "\n";
echo "FRect ", ptr_sizeof("FRect"), " ", elephc_test_struct_size(3), "\n";
echo "All ", ptr_sizeof("All"), " ", elephc_test_struct_size(4), "\n";
"#,
    );
    assert_eq!(
        out,
        "KeyEvent 40 40\nRect 16 16\nColor 4 4\nFRect 16 16\nAll 80 80\n"
    );
}

/// Verifies fields C filled read back by name with their declared width: unsigned fields zero-extend
/// (`which` is 4294967295, `mod` 32769), `u64` passes through, and each `bool` is its own byte.
#[test]
fn test_ffi_extern_class_reads_c_filled_fields_by_name() {
    let out = run_struct_program(
        r#"
$e = elephc_test_key_event_filled();
echo $e->type, " ", $e->reserved, " ", $e->timestamp, "\n";
echo $e->windowID, " ", $e->which, " ", $e->scancode, " ", $e->key, "\n";
echo $e->mod, " ", $e->raw, "\n";
var_dump($e->down, $e->repeat);
"#,
    );
    assert_eq!(
        out,
        "768 3735928559 123456789012345\n7 4294967295 4 97\n32769 65535\nbool(true)\nbool(false)\n"
    );
}

/// Verifies signed narrow fields sign-extend, unsigned ones zero-extend, a `u64` above `PHP_INT_MAX`
/// reads as the same bits, `f32` widens exactly, a `ptr<All>` field chains into the struct it points
/// to, and `char*` fields copy into PHP strings (NULL reads as the empty string).
#[test]
fn test_ffi_extern_class_reads_every_width_with_its_extension() {
    let out = run_struct_program(
        r#"
$a = elephc_test_all_filled();
echo $a->i8, " ", $a->u8, " ", $a->i16, " ", $a->u16, "\n";
echo $a->i32, " ", $a->u32, " ", $a->i64, " ", $a->u64, "\n";
echo $a->f32, " ", $a->f64, "\n";
var_dump($a->flag);
echo $a->self->i32, " ", $a->self->self->u16, "\n";
echo "[", $a->name, "] [", $a->missing, "] ", strlen($a->name), "\n";
"#,
    );
    assert_eq!(
        out,
        "-5 250 -1234 65000\n-100000 4000000000 -5000000000 -1\n1.5 2.25\nbool(true)\n-100000 65000\n[sdl] [] 3\n"
    );
}

/// Verifies writes store only the declared width, so C sees each field exactly as C would have
/// converted the value: out-of-range integers keep their low bits (`300` in a `u8` is `44`),
/// neighbouring narrow fields are not clobbered, and arithmetic results (typed for overflow) are
/// narrowed into integer fields.
#[test]
fn test_ffi_extern_class_writes_truncate_to_the_field_width() {
    let out = run_struct_program(
        r#"
$r = elephc_test_rect_blank();
$r->x = -1;
$r->y = 5000000000;
$r->w = 640;
$r->h = $r->w * 2;
$r->w += 1;
echo elephc_test_describe_rect(), "\n";
$c = elephc_test_color_blank();
$c->r = 300;
$c->g = -1;
$c->b = 128;
$c->a = 255;
echo elephc_test_describe_color(), "\n";
echo $c->r, " ", $c->g, " ", $c->b, " ", $c->a, "\n";
"#,
    );
    assert_eq!(out, "-1 705032704 641 1280\n44 255 128 255\n44 255 128 255\n");
}

/// Verifies `f32` writes round to an IEEE 754 single (C compares against `0.1f` and a denormal),
/// an `int` stored into a float field converts, and every field of the all-widths struct lands
/// where C reads it, including a `bool` cleared to false and a `char*` copied for C.
#[test]
fn test_ffi_extern_class_writes_every_width_where_c_reads_it() {
    let out = run_struct_program(
        r#"
$f = elephc_test_frect_blank();
$f->x = 0.1;
$f->y = 1.0e-45;
$f->w = 3;
$f->h = 2.5;
echo elephc_test_describe_frect(), "\n";
echo $f->x, "\n";
$a = elephc_test_all_blank();
$a->i8 = 200;
$a->u8 = -2;
$a->i16 = 40000;
$a->u16 = -1;
$a->i32 = -7;
$a->u32 = -1;
$a->i64 = PHP_INT_MIN;
$a->u64 = -1;
$a->f32 = 0.75;
$a->f64 = 9.5;
$a->flag = true;
$a->flag = false;
$a->name = "elephc";
echo elephc_test_describe_all(), "\n";
echo $a->name, "\n";
"#,
    );
    assert_eq!(
        out,
        "single denormal 3.00 2.50\n0.10000000149012\n-56 254 -25536 65535 -7 4294967295 -9223372036854775808 18446744073709551615 f32 9.500 0 elephc\nelephc\n"
    );
}

/// Verifies a runtime value that is not an int raises a catchable `TypeError` when stored into an
/// integer field, instead of writing the boxed cell pointer into C memory.
#[test]
fn test_ffi_extern_class_int_field_rejects_a_runtime_float() {
    let out = run_struct_program(
        r#"
$r = elephc_test_rect_blank();
$r->x = 1;
$value = json_decode("2.5");
try {
    $r->x = $value;
} catch (TypeError $e) {
    echo $e->getMessage(), "\n";
}
echo $r->x, "\n";
"#,
    );
    assert_eq!(out, "Extern field Rect::$x must be of type int, float given\n1\n");
}

/// Verifies float and `bool` fields narrow a value whose type is only known at run time, like a
/// packed field: float arithmetic over a loop counter (boxed in EIR although the checker types it
/// `float`) reaches C memory as a float instead of failing to compile, a runtime int converts
/// into a float field, and any other runtime value raises a `TypeError` before the field changes.
#[test]
fn test_ffi_extern_class_float_and_bool_fields_narrow_runtime_values() {
    let out = run_struct_program(
        r#"
$f = elephc_test_frect_blank();
for ($i = 0; $i < 3; $i++) {
    $f->w = 1.5 * ($i + 1);
    $f->h = 0.25 * ($i + 1);
}
echo $f->w, " ", $f->h, " ", elephc_test_describe_frect(), "\n";
$a = elephc_test_all_blank();
$a->f64 = json_decode("3");
$a->f32 = json_decode("0.5");
$a->flag = json_decode("true");
var_dump($a->f64, $a->f32, $a->flag);
try {
    $a->f64 = json_decode("\"x\"");
} catch (TypeError $e) {
    echo $e->getMessage(), "\n";
}
try {
    $a->flag = json_decode("1");
} catch (TypeError $e) {
    echo $e->getMessage(), "\n";
}
var_dump($a->f64, $a->flag);
"#,
    );
    assert_eq!(
        out,
        "4.5 0.75 not-single other 4.50 0.75\nfloat(3)\nfloat(0.5)\nbool(true)\n\
         Extern field All::$f64 must be of type float, string given\n\
         Extern field All::$flag must be of type bool, int given\n\
         float(3)\nbool(true)\n"
    );
}

/// Verifies scalar extern field reads keep their type in array literals and branch merges, where
/// the lowering used to type every property read `int`: floats truncated, a bool read as `1`
/// and a `string` field as `0`.
#[test]
fn test_ffi_extern_class_field_reads_keep_their_type_in_literals_and_merges() {
    let out = run_struct_program(
        r#"
$a = elephc_test_all_filled();
var_dump([$a->f32, $a->f64, $a->flag, $a->u8, $a->name]);
$picked = $argc > 5 ? 0 : $a->f64;
var_dump($picked);
"#,
    );
    assert_eq!(
        out,
        "array(5) {\n  [0]=>\n  float(1.5)\n  [1]=>\n  float(2.25)\n  [2]=>\n  bool(true)\n  [3]=>\n  int(250)\n  [4]=>\n  string(3) \"sdl\"\n}\nfloat(2.25)\n"
    );
}

/// Verifies a field read through a null `ptr<ExternClass>` aborts with the pointer extension's
/// fatal, the same null-receiver semantics packed `ptr<T>` receivers have.
#[test]
fn test_ffi_extern_class_field_read_on_null_is_fatal() {
    let err = compile_and_run_expect_failure(
        r#"<?php
extern class Rect { public i32 $x; public i32 $y; }
$r = ptr_cast<Rect>(ptr_null());
echo $r->y;
"#,
    );
    assert!(err.contains("null pointer dereference"), "{}", err);
}

/// Verifies a field write through a null `ptr<ExternClass>` aborts instead of storing to address zero.
#[test]
fn test_ffi_extern_class_field_write_on_null_is_fatal() {
    let err = compile_and_run_expect_failure(
        r#"<?php
extern class Color { public u8 $r; public u8 $g; }
$c = ptr_cast<Color>(ptr_null());
$c->g = 7;
"#,
    );
    assert!(err.contains("null pointer dereference"), "{}", err);
}

/// Verifies a field past AArch64's scaled-immediate range (a byte field at offset 4160, beyond
/// 4095) is read and written at the right address, cross-checked with raw `ptr_read8`.
#[test]
fn test_ffi_extern_class_field_beyond_the_immediate_offset_range() {
    let words: String = (0..520).map(|index| format!("public int $w{index}; ")).collect();
    let out = compile_and_run(&format!(
        r#"<?php
extern function malloc(u64 $size): ptr;
extern class Big {{ {words}public u8 $tail; public u8 $after; }}
$big = ptr_cast<Big>(malloc(ptr_sizeof("Big")));
$big->w519 = 77;
$big->tail = 513;
$big->after = 9;
echo ptr_sizeof("Big"), " ", $big->tail, " ", ptr_read8(ptr_offset($big, 4160)), " ", $big->after, " ", $big->w519, "\n";
"#
    ));
    assert_eq!(out, "4168 1 1 9 77\n");
}

/// Verifies `ptr<T>` in a namespaced extern class field names the namespace's class, so a chained
/// `$n->next->value` read compiles and reads that struct.
#[test]
fn test_ffi_extern_class_ptr_targets_resolve_in_a_namespace() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
extern function calloc(u64 $count, u64 $size): ptr;
require "node.php";
$head = ptr_cast<App\Node>(calloc(1, ptr_sizeof("App\\Node")));
$tail = ptr_cast<App\Node>(calloc(1, ptr_sizeof("App\\Node")));
App\link($head, $tail);
echo App\second($head), " ", ptr_sizeof("App\\Node"), "\n";
"#,
            ),
            (
                "node.php",
                r#"<?php
namespace App;
extern class Node {
    public i32 $value;
    public ptr<Node> $next;
}
function link(ptr<Node> $head, ptr<Node> $tail): void {
    $tail->value = 41;
    $tail->value += 1;
    $head->next = $tail;
}
function second(ptr<Node> $n): int {
    return $n->next->value;
}
"#,
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "42 16\n");
}

/// Verifies a namespaced extern class wins over a global one with the same short name: the
/// `ptr<Node>` field inside `namespace App` must use `App\Node`'s offsets, not the global
/// struct's, which `ptr_read32` at both candidate offsets tells apart.
#[test]
fn test_ffi_extern_class_ptr_targets_do_not_bind_a_same_named_global_class() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
extern function calloc(u64 $count, u64 $size): ptr;
extern class Node {
    public i64 $pad;
    public ptr<Node> $next;
    public i32 $value;
}
require "node.php";
$head = ptr_cast<App\Node>(calloc(1, 32));
$tail = ptr_cast<App\Node>(calloc(1, 32));
$tail->value = 7;
$head->next = $tail;
echo App\second($head), " ", ptr_read32($tail), " ", ptr_read32(ptr_offset($tail, 16)), " ";
echo ptr_sizeof("Node"), " ", ptr_sizeof("App\\Node"), "\n";
"#,
            ),
            (
                "node.php",
                r#"<?php
namespace App;
extern class Node {
    public i32 $value;
    public ptr<Node> $next;
}
function second(ptr<Node> $n): int {
    return $n->next->value;
}
"#,
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "7 7 0 24 16\n");
}

/// Verifies a literal dynamic name (`$r->{'y'}`) reads a C struct field like `$r->y`: the narrow
/// width is sign-extended, a `char*` field is copied into a PHP string, and a null receiver
/// aborts with the null-dereference fatal.
#[test]
fn test_ffi_extern_class_literal_dynamic_name_reads_the_field() {
    let out = compile_and_run(
        r#"<?php
extern function calloc(u64 $count, u64 $size): ptr;
extern class Tag { public i32 $id; public i16 $delta; public string $name; }
$t = ptr_cast<Tag>(calloc(1, ptr_sizeof("Tag")));
$t->delta = -3;
$t->name = "left";
$name = $t->{'name'};
$t->name = "right";
echo $t->{'delta'}, " ", $name, " ", $t->{'name'}, "\n";
"#,
    );
    assert_eq!(out, "-3 left right\n");
    let err = compile_and_run_expect_failure(
        r#"<?php
extern class Tag { public i32 $id; public i16 $delta; }
function delta(ptr<Tag> $t): int { return $t->{'delta'}; }
echo delta(ptr_cast<Tag>(ptr_null()));
"#,
    );
    assert!(err.contains("null pointer dereference"), "{}", err);
}

/// Verifies a packed class named only as the `ptr<T>` type of an extern class field keeps its
/// layout after declaration pruning, so `$h->p->x` reads and writes through the packed record.
#[test]
fn test_ffi_extern_class_pointer_field_keeps_its_packed_target() {
    let out = compile_and_run(
        r#"<?php
extern function calloc(u64 $count, u64 $size): ptr;
packed class Pair { public int $x; public int $y; }
extern class Holder { public i32 $tag; public ptr<Pair> $p; }
$h = ptr_cast<Holder>(calloc(1, ptr_sizeof("Holder")));
$h->p = calloc(1, 16);
$h->p->x = 41;
$h->p->y = 1;
echo $h->p->x + $h->p->y, "\n";
"#,
    );
    assert_eq!(out, "42\n");
}

/// Verifies `isset()`, `empty()` and `??` with a literal dynamic name (`$r->{'y'}`) probe a C
/// struct field exactly like their named forms: the same answers, an owned copy of a `char*`
/// field, and the null-dereference fatal on a null receiver.
#[test]
fn test_ffi_extern_class_literal_dynamic_name_probes_the_field() {
    let out = compile_and_run(
        r#"<?php
extern function calloc(u64 $count, u64 $size): ptr;
extern class Tag { public i32 $id; public i16 $delta; public string $name; }
$t = ptr_cast<Tag>(calloc(1, ptr_sizeof("Tag")));
$t->delta = -3;
$t->name = "left";
echo isset($t->delta) ? "set" : "unset", " ", empty($t->id) ? "empty" : "full", " ",
    empty($t->delta) ? "empty" : "full", " ", $t->delta ?? 9, "\n";
echo isset($t->{'delta'}) ? "set" : "unset", " ", empty($t->{'id'}) ? "empty" : "full", " ",
    empty($t->{'delta'}) ? "empty" : "full", " ", $t->{'delta'} ?? 9, "\n";
$name = $t->{'name'} ?? "none";
$t->name = "right";
echo $name, " ", $t->{'name'} ?? "none", "\n";
"#,
    );
    assert_eq!(out, "set empty full -3\nset empty full -3\nleft right\n");
    let err = compile_and_run_expect_failure(
        r#"<?php
extern class Tag { public i32 $id; public i16 $delta; }
function has_delta(ptr<Tag> $t): bool { return isset($t->{'delta'}); }
echo has_delta(ptr_cast<Tag>(ptr_null())) ? "set" : "unset";
"#,
    );
    assert!(err.contains("null pointer dereference"), "{}", err);
}
