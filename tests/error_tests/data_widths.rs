//! Purpose:
//! Diagnostic tests for C storage widths in packed classes and `buffer<T>` elements: width names
//! are storage formats, not types, and a width-declared field or element takes exactly what its
//! `int` or `float` value type takes.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Invalid PHP snippets are checked through shared diagnostic helpers for messages.

use super::*;

/// Verifies a width name is not a type outside packed fields and buffer elements: a parameter
/// declared `i32` is an unknown type, as before.
#[test]
fn test_error_width_names_are_not_types_outside_packed_and_buffer() {
    expect_error(
        "<?php function f(i32 $x): int { return $x; }",
        "Unknown type: i32",
    );
}

/// Verifies `buffer<i32>` and `buffer<int>` are distinct types: their strides differ, so one
/// cannot initialize a local declared as the other.
#[test]
fn test_error_width_buffer_is_distinct_from_int_buffer() {
    expect_error(
        "<?php buffer<int> $values = buffer_new<i32>(2);",
        "cannot initialize $values",
    );
}

/// Verifies an integer-width packed field takes an `int`, like a plain `int` field.
#[test]
fn test_error_width_packed_field_rejects_a_float() {
    expect_error(
        "<?php packed class P { public i32 $x; } $b = buffer_new<P>(1); $b[0]->x = 1.5;",
        "cannot assign float to packed field P::x of type int",
    );
}

/// Verifies an `f32` packed field takes a `float`, like a plain `float` field.
#[test]
fn test_error_f32_packed_field_rejects_a_string() {
    expect_error(
        "<?php packed class P { public f32 $x; } $b = buffer_new<P>(1); $b[0]->x = \"a\";",
        "cannot assign string to packed field P::x of type float",
    );
}

/// Verifies an `f32` buffer element takes a `float`, like a `buffer<float>` element.
#[test]
fn test_error_f32_buffer_element_rejects_an_int() {
    expect_error(
        "<?php buffer<f32> $v = buffer_new<f32>(2); $v[0] = 1;",
        "Buffer element type mismatch: expected float, got int",
    );
}

/// Verifies a width is only a bare field type: a nullable width is not a storage format.
#[test]
fn test_error_width_packed_field_cannot_be_nullable() {
    expect_error("<?php packed class P { public ?i32 $x; }", "Unknown type: i32");
}

/// Verifies an integer-width buffer element takes an `int`, like a `buffer<int>` element.
#[test]
fn test_error_u8_buffer_element_rejects_a_float() {
    expect_error(
        "<?php buffer<u8> $b = buffer_new<u8>(2); $b[0] = 1.5;",
        "Buffer element type mismatch: expected int, got float",
    );
}

/// Verifies an integer-width packed field takes an `int`, not a `bool`.
#[test]
fn test_error_width_packed_field_rejects_a_bool() {
    expect_error(
        "<?php packed class P { public u8 $f; } $b = buffer_new<P>(1); $b[0]->f = true;",
        "cannot assign bool to packed field P::f of type int",
    );
}

/// Verifies `buffer<i32>` is not accepted where a `buffer<int>` parameter is declared.
#[test]
fn test_error_width_buffer_rejected_at_an_int_buffer_parameter() {
    expect_error(
        "<?php function f(buffer<int> $b): void {} f(buffer_new<i32>(2));",
        "expects buffer<int>, got buffer<i32>",
    );
}

/// Verifies `buffer<i32>` is not accepted where a `buffer<int>` return type is declared.
#[test]
fn test_error_width_buffer_rejected_at_an_int_buffer_return() {
    expect_error(
        "<?php function g(): buffer<int> { return buffer_new<i32>(2); }",
        "buffer<i32>",
    );
}

/// Verifies a width is only a bare element type: nullable and union spellings are not widths.
#[test]
fn test_error_width_buffer_element_cannot_be_nullable_or_union() {
    expect_error("<?php buffer<?u8> $b = buffer_new<?u8>(2);", "Unknown type: u8");
    expect_error("<?php buffer<u8|int> $b = buffer_new<u8|int>(2);", "Unknown type: u8");
}

/// Verifies a misspelled width in a packed field or buffer element names the accepted
/// spellings, inside and outside a namespace, instead of pointing at a missing class.
#[test]
fn test_error_misspelled_width_suggests_the_spelling() {
    expect_error("<?php packed class P { public I32 $x; }", "did you mean i32?");
    expect_error(
        "<?php namespace G; packed class V { public uint8_t $f; }",
        "did you mean u8?",
    );
    expect_error(
        "<?php buffer<Int32> $b = buffer_new<Int32>(2);",
        "storage widths are spelled i8, u8, i16, u16, i32, u32, i64, u64, f32, f64 (did you mean i32?)",
    );
}

/// Verifies a class or `use` alias named like a width shadows the width where it is visible,
/// so the type hint resolves to that class as in PHP.
#[test]
fn test_class_or_alias_named_like_a_width_resolves_as_a_class() {
    expect_no_error(
        "<?php namespace App; final class u8 { public function __construct(public int $v) {} } function show(u8 $x): int { return $x->v; } echo show(new u8(5));",
    );
    expect_no_error(
        "<?php namespace Lib { final class Byte { public int $v = 1; } } namespace App { use Lib\\Byte as i32; function f(i32 $b): int { return $b->v; } echo f(new \\Lib\\Byte()); }",
    );
}

/// Verifies a packed class named like a width shadows the width where it is visible: a `u8`
/// field of another packed class then embeds that class, and a genuine embedding cycle through
/// it is still reported.
#[test]
fn test_packed_class_named_like_a_width_is_embedded_as_a_class() {
    expect_no_error(
        "<?php packed class u8 { public int $v; } packed class A { public u8 $x; } echo ptr_sizeof(\"A\");",
    );
    expect_error(
        "<?php packed class A { public u8 $x; } packed class u8 { public A $a; }",
        "embeds itself through its packed class fields",
    );
}
