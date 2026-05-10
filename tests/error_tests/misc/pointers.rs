//! Purpose:
//! Integration or regression tests for diagnostic coverage of misc pointers, including ptr no args, ptr requires variable argument, and ptr null with args.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Invalid PHP snippets are checked through shared diagnostic helpers for messages, spans, and recovery behavior.

use super::*;

#[test]
fn test_error_ptr_no_args() {
    expect_error("<?php ptr();", "ptr() takes exactly 1 argument");
}

#[test]
fn test_error_ptr_requires_variable_argument() {
    expect_error("<?php ptr(1 + 2);", "ptr() argument must be a variable");
}

#[test]
fn test_error_ptr_null_with_args() {
    expect_error("<?php ptr_null(1);", "ptr_null() takes 0 arguments");
}

#[test]
fn test_error_ptr_is_null_wrong_args() {
    expect_error(
        "<?php ptr_is_null();",
        "ptr_is_null() takes exactly 1 argument",
    );
}

#[test]
fn test_error_ptr_is_null_requires_pointer() {
    expect_error(
        "<?php ptr_is_null(123);",
        "ptr_is_null() requires a pointer argument",
    );
}

#[test]
fn test_error_ptr_offset_wrong_args() {
    expect_error(
        "<?php $p = ptr_null(); ptr_offset($p);",
        "ptr_offset() takes exactly 2 arguments",
    );
}

#[test]
fn test_error_ptr_offset_requires_pointer() {
    expect_error(
        "<?php ptr_offset(123, 8);",
        "ptr_offset() requires a pointer argument",
    );
}

#[test]
fn test_error_ptr_offset_requires_integer_offset() {
    expect_error(
        "<?php $p = ptr_null(); ptr_offset($p, \"8\");",
        "ptr_offset() second argument must be integer",
    );
}

#[test]
fn test_error_ptr_get_wrong_args() {
    expect_error("<?php ptr_get();", "ptr_get() takes exactly 1 argument");
}

#[test]
fn test_error_ptr_get_requires_pointer() {
    expect_error(
        "<?php ptr_get(123);",
        "ptr_get() requires a pointer argument",
    );
}

#[test]
fn test_error_ptr_read8_requires_pointer() {
    expect_error(
        "<?php ptr_read8(123);",
        "ptr_read8() requires a pointer argument",
    );
}

#[test]
fn test_error_ptr_read32_requires_pointer() {
    expect_error(
        "<?php ptr_read32(123);",
        "ptr_read32() requires a pointer argument",
    );
}

#[test]
fn test_error_ptr_set_wrong_args() {
    expect_error(
        "<?php ptr_set(ptr_null());",
        "ptr_set() takes exactly 2 arguments",
    );
}

#[test]
fn test_error_ptr_set_requires_pointer() {
    expect_error(
        "<?php ptr_set(123, 1);",
        "ptr_set() requires a pointer argument",
    );
}

#[test]
fn test_error_ptr_write8_requires_int_value() {
    expect_error(
        "<?php $p = ptr_null(); ptr_write8($p, \"hello\");",
        "ptr_write8() value must be int",
    );
}

#[test]
fn test_error_ptr_write32_requires_int_value() {
    expect_error(
        "<?php $p = ptr_null(); ptr_write32($p, \"hello\");",
        "ptr_write32() value must be int",
    );
}

#[test]
fn test_error_ptr_sizeof_wrong_args() {
    expect_error(
        "<?php ptr_sizeof();",
        "ptr_sizeof() takes exactly 1 argument",
    );
}

#[test]
fn test_error_ptr_sizeof_requires_literal() {
    expect_error(
        "<?php $t = \"int\"; ptr_sizeof($t);",
        "ptr_sizeof() argument must be a string literal",
    );
}

#[test]
fn test_error_ptr_sizeof_unknown_type() {
    expect_error(
        "<?php ptr_sizeof(\"NoSuchType\");",
        "Unknown type for ptr_sizeof(): NoSuchType",
    );
}

#[test]
fn test_error_ptr_cast_missing_type() {
    expect_error(
        "<?php ptr_cast<>(ptr_null());",
        "Expected type name after 'ptr_cast<'",
    );
}

#[test]
fn test_error_ptr_cast_requires_pointer_argument() {
    expect_error(
        "<?php ptr_cast<int>(123);",
        "ptr_cast() requires a pointer argument",
    );
}

#[test]
fn test_error_ptr_cast_rejects_unknown_target() {
    expect_error(
        "<?php $p = ptr_null(); ptr_cast<NoSuchType>($p);",
        "Unknown ptr_cast target type: NoSuchType",
    );
}
