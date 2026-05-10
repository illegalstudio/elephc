//! Purpose:
//! Integration or regression tests for diagnostic coverage of extensions, including packed class rejects non pod field, buffer new rejects non pod element type, and buffer new rejects union element type.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Invalid PHP snippets are checked through shared diagnostic helpers for messages, spans, and recovery behavior.

use super::*;

#[test]
fn test_error_packed_class_rejects_non_pod_field() {
    expect_error(
        "<?php packed class Bad { public string $name; }",
        "Packed class fields must use POD scalars, pointers, or packed classes",
    );
}

#[test]
fn test_error_buffer_new_rejects_non_pod_element_type() {
    expect_error(
        "<?php buffer<string> $names = buffer_new<string>(2);",
        "buffer<T> requires a POD scalar, pointer, or packed class element type",
    );
}

#[test]
fn test_error_buffer_new_rejects_union_element_type() {
    expect_error(
        "<?php buffer<int|string> $values = buffer_new<int|string>(2);",
        "buffer<T> requires a POD scalar, pointer, or packed class element type",
    );
}

#[test]
fn test_error_packed_class_rejects_nullable_field() {
    expect_error(
        "<?php packed class MaybePoint { public ?int $x; }",
        "Packed class fields must use POD scalars, pointers, or packed classes",
    );
}

#[test]
fn test_error_buffer_scalar_assign_type_mismatch() {
    expect_error(
        "<?php buffer<int> $values = buffer_new<int>(2); $values[0] = true;",
        "Buffer element type mismatch",
    );
}

#[test]
fn test_error_buffer_packed_element_requires_field_assignment() {
    expect_error(
        "<?php packed class Vec2 { public float $x; public float $y; } buffer<Vec2> $points = buffer_new<Vec2>(1); $points[0] = 1;",
        "Assign packed buffer elements through field access like $buf[$i]->field",
    );
}

#[test]
fn test_error_buffer_len_requires_buffer_argument() {
    expect_error(
        "<?php echo buffer_len(1);",
        "buffer_len() argument must be buffer<T>",
    );
}

#[test]
fn test_error_buffer_free_requires_buffer_argument() {
    expect_error(
        "<?php buffer_free(42);",
        "buffer_free() argument must be buffer<T>",
    );
}

#[test]
fn test_error_buffer_free_wrong_arg_count() {
    expect_error(
        "<?php buffer<int> $b = buffer_new<int>(1); buffer_free($b, $b);",
        "buffer_free() takes exactly 1 argument",
    );
}

#[test]
fn test_error_buffer_free_requires_local_variable() {
    expect_error(
        "<?php buffer_free(buffer_new<int>(1));",
        "buffer_free() argument must be a local variable",
    );
}

#[test]
fn test_error_buffer_free_rejects_ref_param() {
    expect_error(
        "<?php function drop(&$buf) { buffer_free($buf); } buffer<int> $buf = buffer_new<int>(1); drop($buf);",
        "buffer_free() argument must be a local variable",
    );
}

#[test]
fn test_error_buffer_free_rejects_global_alias() {
    expect_error(
        "<?php buffer<int> $buf = buffer_new<int>(1); function drop() { global $buf; buffer_free($buf); } drop();",
        "buffer_free() argument must be a local variable",
    );
}

#[test]
fn test_error_buffer_free_rejects_static_slot() {
    expect_error(
        "<?php function drop() { static $buf = buffer_new<int>(1); buffer_free($buf); } drop();",
        "buffer_free() argument must be a local variable",
    );
}

#[test]
fn test_error_extern_unknown_type() {
    expect_error(
        "<?php extern function foo(badtype $x): int;",
        "Unknown C type: badtype",
    );
}

#[test]
fn test_error_extern_block_empty() {
    expect_error("<?php extern \"lib\" { }", "Empty extern block");
}

#[test]
fn test_error_extern_wrong_arg_count() {
    expect_error(
        "<?php extern function abs(int $n): int; abs();",
        "Extern function 'abs' expects 1 arguments, got 0",
    );
}

#[test]
fn test_error_extern_wrong_arg_type() {
    expect_error(
        "<?php extern function strlen(string $s): int; strlen(123);",
        "Extern function 'strlen' parameter $s expects Str, got Int",
    );
}

#[test]
fn test_error_duplicate_extern_function() {
    expect_error(
        "<?php extern function foo(int $x): int; extern function foo(int $y): int;",
        "Duplicate function declaration: foo",
    );
}

#[test]
fn test_error_extern_global_reserved_name() {
    expect_error(
        "<?php extern global int $argc;",
        "extern global $argc would shadow a reserved superglobal",
    );
}

#[test]
fn test_error_extern_global_void_type() {
    expect_error(
        "<?php extern global void $bad;",
        "Extern global $bad uses an unsupported type",
    );
}

#[test]
fn test_error_extern_callable_requires_literal_function_name() {
    expect_error(
        "<?php extern function signal(int $sig, callable $handler): ptr; function on_signal($sig) {} $fn = \"on_signal\"; signal(15, $fn);",
        "expects a string literal naming a user function",
    );
}

#[test]
fn test_error_extern_callable_requires_defined_function() {
    expect_error(
        "<?php extern function signal(int $sig, callable $handler): ptr; signal(15, \"missing_handler\");",
        "Undefined callback function: missing_handler",
    );
}

#[test]
fn test_error_extern_callable_requires_c_compatible_return_type() {
    expect_error(
        "<?php extern function signal(int $sig, callable $handler): ptr; function bad_handler($sig) { return \"oops\"; } signal(15, \"bad_handler\");",
        "unsupported return type",
    );
}

#[test]
fn test_error_extern_class_void_field() {
    expect_error(
        "<?php extern class Bad { void $field; }",
        "Extern class 'Bad' field $field uses an unsupported type",
    );
}
