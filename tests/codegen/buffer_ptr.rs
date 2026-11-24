//! Purpose:
//! End-to-end tests for `buffer_ptr()`, the payload address of a `buffer<T>`.
//!
//! Called from:
//! - `cargo test --test codegen_tests_runtime buffer_ptr` through Rust's test harness.
//!
//! Key details:
//! - Scalar and width buffers are read and written through the raw pointer builtins at their
//!   element stride, and handed to a C fixture as `const uint16_t*`, `float*` and `uint8_t*`.
//! - A buffer of packed records answers `ptr<T>`, the address `$buf[0]` reads.
//! - A freed handle reports the use-after-free diagnostic like every other buffer operation.

use crate::support::*;

/// C side of the FFI test: sums a `uint16_t` index list, scales a `float` stream in place and
/// fills a byte buffer, each through the payload pointer PHP hands over.
const PAYLOAD_FIXTURE: &str = r#"
#include <stdint.h>

long elephc_test_sum_u16(const uint16_t *p, long n) {
    long sum = 0;
    for (long i = 0; i < n; i++) {
        sum += p[i];
    }
    return sum;
}

void elephc_test_scale_f32(float *p, long n, float k) {
    for (long i = 0; i < n; i++) {
        p[i] *= k;
    }
}

void elephc_test_fill_u8(uint8_t *p, long n, int first) {
    for (long i = 0; i < n; i++) {
        p[i] = (uint8_t)(first + i);
    }
}
"#;

/// Verifies `buffer_ptr()` of a `buffer<int>` addresses the first element and that the pointer
/// and the index syntax observe the same memory in both directions.
#[test]
fn test_buffer_ptr_addresses_the_scalar_payload() {
    let out = compile_and_run(
        r#"<?php
buffer<int> $b = buffer_new<int>(3);
$b[0] = 10;
$b[1] = 20;
$b[2] = 30;
$p = buffer_ptr($b);
echo ptr_get($p), " ", ptr_get(ptr_offset($p, 8)), " ", ptr_get(ptr_offset($p, 16)), "\n";
ptr_set(ptr_offset($p, 8), 99);
echo $b[1], " ", ptr_is_null($p) ? "null" : "live", "\n";
buffer_free($b);
"#,
    );
    assert_eq!(out, "10 20 30\n99 live\n");
}

/// Verifies the payload of a width buffer keeps the element stride: `u16` elements are 2 bytes
/// apart, `u8` elements 1, `f32` and `i32` elements 4, and raw writes through the pointer read
/// back through the index syntax with the width's truncation.
#[test]
fn test_buffer_ptr_width_buffers_keep_their_stride() {
    let out = compile_and_run(
        r#"<?php
buffer<u16> $idx = buffer_new<u16>(4);
$idx[0] = 1;
$idx[1] = 2;
$idx[2] = 70000;
$idx[3] = 65535;
$p = buffer_ptr($idx);
echo ptr_read16($p), " ", ptr_read16(ptr_offset($p, 2)), " ", ptr_read16(ptr_offset($p, 4)), " ", ptr_read16(ptr_offset($p, 6)), "\n";
ptr_write16(ptr_offset($p, 2), 4660);
echo $idx[1], "\n";
buffer<u8> $px = buffer_new<u8>(3);
$px[2] = 255;
$q = buffer_ptr($px);
ptr_write8(ptr_offset($q, 1), 300);
echo ptr_read8(ptr_offset($q, 2)), " ", $px[1], "\n";
buffer<f32> $xs = buffer_new<f32>(2);
$xs[1] = 0.5;
$f = buffer_ptr($xs);
ptr_write_f32($f, 1.25);
echo ptr_read_f32(ptr_offset($f, 4)), " ", $xs[0], "\n";
buffer<i32> $is = buffer_new<i32>(2);
$is[0] = 7;
$i = buffer_ptr($is);
ptr_write32(ptr_offset($i, 4), 9);
echo ptr_read32($i), " ", $is[1], "\n";
"#,
    );
    assert_eq!(out, "1 2 4464 65535\n4660\n255 44\n0.5 1.25\n7 9\n");
}

/// Verifies scalar and width buffers reach C through `buffer_ptr()`: C sums a `uint16_t` index
/// list, scales a `float` stream in place and fills a byte buffer, and PHP sees the writes
/// through the index syntax.
#[test]
fn test_buffer_ptr_hands_scalar_buffers_to_c() {
    let out = compile_and_run_with_c_fixture(
        r#"<?php
extern function elephc_test_sum_u16(ptr $p, int $n): int;
extern function elephc_test_scale_f32(ptr $p, int $n, f32 $k): void;
extern function elephc_test_fill_u8(ptr $p, int $n, i32 $first): void;
buffer<u16> $idx = buffer_new<u16>(6);
$order = [0, 1, 2, 0, 2, 3];
for ($i = 0; $i < 6; $i++) {
    $idx[$i] = $order[$i];
}
echo elephc_test_sum_u16(buffer_ptr($idx), buffer_len($idx)), "\n";
buffer<f32> $xs = buffer_new<f32>(3);
$xs[0] = 1.5;
$xs[1] = -2.0;
$xs[2] = 0.25;
elephc_test_scale_f32(buffer_ptr($xs), 3, 4.0);
echo $xs[0], " ", $xs[1], " ", $xs[2], "\n";
buffer<u8> $px = buffer_new<u8>(4);
elephc_test_fill_u8(buffer_ptr($px), buffer_len($px), 253);
echo $px[0], " ", $px[1], " ", $px[2], " ", $px[3], "\n";
"#,
        PAYLOAD_FIXTURE,
    );
    assert_eq!(out, "8\n6 -8 1\n253 254 255 0\n");
}

/// Verifies `buffer_ptr()` of a buffer of packed records is the `ptr<T>` of element 0: the same
/// address `$buf[0]` reads, usable for field access, offset to the next record, and as a typed
/// return value.
#[test]
fn test_buffer_ptr_of_a_packed_buffer_is_the_first_element() {
    let out = compile_and_run(
        r#"<?php
packed class Vec2 { public f32 $x; public f32 $y; }
function origin(buffer<Vec2> $b): ptr<Vec2> { return buffer_ptr($b); }
buffer<Vec2> $pts = buffer_new<Vec2>(3);
$pts[1]->x = 2.5;
$pts[1]->y = -1.0;
$first = buffer_ptr($pts);
echo $first === $pts[0] ? "same" : "different", "\n";
$second = ptr_offset($first, ptr_sizeof("Vec2"));
echo $second->x, " ", $second->y, "\n";
$first->x = 7.0;
echo $pts[0]->x, " ", origin($pts)->x, "\n";
buffer_free($pts);
"#,
    );
    assert_eq!(out, "same\n2.5 -1\n7 7\n");
}

/// Verifies `buffer_ptr()` resolves through a buffer property, a fully qualified call inside a
/// namespace, a case-insensitive call in a closure, and `function_exists()`.
#[test]
fn test_buffer_ptr_through_properties_closures_and_namespaces() {
    let out = compile_and_run(
        r#"<?php
namespace App\Gfx;
final class Mesh {
    public buffer<u16> $indices;
    public function __construct(int $n) { $this->indices = buffer_new<u16>($n); }
    public function indexData(): ptr { return \buffer_ptr($this->indices); }
}
$m = new Mesh(2);
$m->indices[0] = 500;
$m->indices[1] = 600;
$p = $m->indexData();
echo ptr_read16($p), " ", ptr_read16(ptr_offset($p, 2)), "\n";
$fn = fn(buffer<u16> $b): ptr => BUFFER_PTR($b);
echo ptr_read16($fn($m->indices)), "\n";
echo function_exists("buffer_ptr") ? "yes" : "no", "\n";
"#,
    );
    assert_eq!(out, "500 600\n500\nyes\n");
}

/// Verifies the first-class callables `buffer_ptr(...)` and `buffer_len(...)` take every buffer
/// type: their wrapper parameter is the any-buffer handle, so a callable forwarded through a
/// declared `callable` parameter (where only the wrapper signature is known) accepts a width
/// buffer and a buffer of packed records, as does a callable local. Through the callable the
/// pointer is the untyped `ptr`, read with the raw pointer builtins.
#[test]
fn test_buffer_ptr_and_buffer_len_as_first_class_callables_take_any_buffer() {
    let out = compile_and_run(
        r#"<?php
packed class Vec2 { public f32 $x; public f32 $y; }
function indexPayload(callable $f, buffer<u16> $b): ptr { return $f($b); }
function recordPayload(callable $f, buffer<Vec2> $b): ptr { return $f($b); }
function indexLength(callable $f, buffer<u16> $b): int { return $f($b); }
function recordLength(callable $f, buffer<Vec2> $b): int { return $f($b); }
buffer<u16> $idx = buffer_new<u16>(2);
$idx[1] = 700;
buffer<Vec2> $pts = buffer_new<Vec2>(1);
$pts[0]->y = 2.5;
echo ptr_read16(ptr_offset(indexPayload(buffer_ptr(...), $idx), 2)), " ", indexLength(buffer_len(...), $idx), "\n";
echo ptr_read_f32(ptr_offset(recordPayload(buffer_ptr(...), $pts), 4)), " ", recordLength(buffer_len(...), $pts), "\n";
$addr = buffer_ptr(...);
$len = buffer_len(...);
echo ptr_read16(ptr_offset($addr($idx), 2)), " ", $len($idx), " ", ptr_read_f32(ptr_offset($addr($pts), 4)), " ", $len($pts), "\n";
"#,
    );
    assert_eq!(out, "700 2\n2.5 1\n700 2 2.5 1\n");
}

/// Verifies `buffer_ptr()` of a freed buffer reports the use-after-free diagnostic instead of
/// answering a dangling payload address.
#[test]
fn test_buffer_ptr_after_free_is_fatal() {
    let err = compile_and_run_expect_failure(
        r#"<?php
buffer<u8> $buf = buffer_new<u8>(5);
$alias = $buf;
buffer_free($buf);
echo ptr_is_null(buffer_ptr($alias)) ? "null" : "live";
"#,
    );
    assert!(err.contains("use of buffer after buffer_free()"), "{}", err);
}

/// Verifies native pointer output writes and reads use pointer-sized buffer slots.
#[test]
fn test_buffer_ptr_native_pointer_output_round_trip() {
    let out = compile_and_run_with_c_fixture(
        r#"<?php
extern function elephc_test_pointer_output(ptr<ptr<u8>> $out, ptr<u32> $length): void;
function pointer_output_slot(ptr<ptr<u8>> $out): ptr<ptr<u8>> { return $out; }
buffer<ptr<u8>> $slots = buffer_new<ptr<u8>>(2);
buffer<u32> $length = buffer_new<u32>(1);
elephc_test_pointer_output(pointer_output_slot(buffer_ptr($slots)), buffer_ptr($length));
echo ptr_read8($slots[0]), " ", $length[0], " ", ptr_is_null($slots[1]) ? "null" : "set", "\n";
buffer_free($slots);
buffer_free($length);
"#,
        r#"#include <stdint.h>
static uint8_t bytes[] = { 73, 74 };
void elephc_test_pointer_output(uint8_t **out, uint32_t *length) {
    *out = bytes;
    *length = 2;
}
"#,
    );
    assert_eq!(out, "73 2 null\n");
}

/// Autoloads a class mentioned only in a nested pointer signature.
#[test]
fn test_nested_pointer_autoloads_target_class() {
    let out = compile_and_run_files(
        &[
            ("composer.json", r#"{"autoload":{"psr-4":{"App\\":"src/"}}}"#),
            ("src/Node.php", "<?php namespace App; class Node {}"),
            ("main.php", "<?php namespace App; function out(ptr<ptr<Node>> $p): void { echo 42; } out(ptr_null());"),
        ],
        "main.php",
    );
    assert_eq!(out, "42");
}

/// Keeps a namespaced packed pointer target when a native function writes it through a slot.
#[test]
fn test_nested_pointer_preserves_packed_target() {
    let out = compile_and_run_with_c_fixture(
        r#"<?php
namespace App;
packed class Pair { public i32 $value; }
extern function elephc_test_pair_output(ptr<ptr<Pair>> $out): void;
buffer<ptr<Pair>> $slot = buffer_new<ptr<Pair>>(1);
elephc_test_pair_output(buffer_ptr($slot));
echo $slot[0]->value;
buffer_free($slot);
"#,
        r#"#include <stdint.h>
struct Pair { int32_t value; };
static struct Pair pair = { 71 };
void elephc_test_pair_output(struct Pair **out) { *out = &pair; }
"#,
    );
    assert_eq!(out, "71");
}

/// Accepts a width pointer in a packed field, formerly rejected as an unknown target.
#[test]
fn test_packed_field_accepts_width_pointer() {
    let out = compile_and_run(r#"<?php
packed class ByteSlot { public ptr<u8> $value; }
$slots = buffer_new<ByteSlot>(1);
echo ptr_is_null($slots[0]->value) ? "null" : "set";
buffer_free($slots);
"#);
    assert_eq!(out, "null");
}

/// Resolves width pointer targets in a namespace, including nested declarations.
#[test]
fn test_namespaced_width_pointer_targets() {
    let out = compile_and_run(r#"<?php
namespace App;
packed class Slot { public ptr<u8> $value; }
function accept(ptr<ptr<u8>> $value): void { echo "ok"; }
accept(ptr_null());
"#);
    assert_eq!(out, "ok");
}

/// A class named like a width remains the pointee of a namespaced pointer type.
#[test]
fn test_namespaced_pointer_target_width_shadowed_by_class() {
    let out = compile_and_run(r#"<?php
namespace App;
packed class u8 { public i32 $value; }
function read(ptr<u8> $p): int { return $p->value; }
$items = buffer_new<u8>(1);
$items[0]->value = 5;
echo read(buffer_ptr($items));
buffer_free($items);
"#);
    assert_eq!(out, "5");
}

/// A rooted width spelling still resolves to a declared global class of that name.
#[test]
fn test_rooted_width_pointer_target_declared_class() {
    let out = compile_and_run(r#"<?php
packed class u8 { public i32 $value; }
function read_rooted(ptr<\u8> $p): int { return $p->value; }
function accept_rooted_nested(ptr<ptr<\u8>> $p): void {}
$items = buffer_new<u8>(1);
$items[0]->value = 9;
accept_rooted_nested(ptr_null());
echo read_rooted(buffer_ptr($items));
buffer_free($items);
"#);
    assert_eq!(out, "9");
}

/// Generic pointer arguments preserve a rooted global class whose name is a C width.
#[test]
fn test_generic_rooted_width_pointer_target_declared_class() {
    let out = compile_and_run(r#"<?php
packed class u8 { public i32 $value; }
function id<T>(T $x): T { return $x; }
$items = buffer_new<u8>(1);
$items[0]->value = 17;
$p = id<ptr<\u8>>(buffer_ptr($items));
echo $p->value;
buffer_free($items);
"#);
    assert_eq!(out, "17");
}

/// Casting to a global class named like a width accepts both rooted and bare names.
#[test]
fn test_ptr_cast_width_named_global_class() {
    let out = compile_and_run(r#"<?php
packed class u8 { public i32 $value; }
$items = buffer_new<u8>(1);
$items[0]->value = 3;
echo ptr_cast<u8>(buffer_ptr($items))->value, ptr_cast<\u8>(buffer_ptr($items))->value;
buffer_free($items);
"#);
    assert_eq!(out, "33");
}
