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
// Tests that `ptr()` with no arguments produces "ptr() takes exactly 1 argument".
fn test_error_ptr_no_args() {
    expect_error("<?php ptr();", "ptr() takes exactly 1 argument");
}

#[test]
// Tests that `ptr()` with a non-variable expression (e.g. `1 + 2`) produces "ptr() argument must be a variable".
fn test_error_ptr_requires_variable_argument() {
    expect_error("<?php ptr(1 + 2);", "ptr() argument must be a variable");
}

#[test]
// Tests that `ptr_null()` with arguments produces "ptr_null() takes 0 arguments".
fn test_error_ptr_null_with_args() {
    expect_error("<?php ptr_null(1);", "ptr_null() takes 0 arguments");
}

#[test]
// Tests that `ptr_is_null()` with no arguments produces "ptr_is_null() takes exactly 1 argument".
fn test_error_ptr_is_null_wrong_args() {
    expect_error(
        "<?php ptr_is_null();",
        "ptr_is_null() takes exactly 1 argument",
    );
}

#[test]
// Tests that `ptr_is_null()` with a non-pointer argument (e.g. `123`) produces "ptr_is_null() requires a pointer argument".
fn test_error_ptr_is_null_requires_pointer() {
    expect_error(
        "<?php ptr_is_null(123);",
        "ptr_is_null() requires a pointer argument",
    );
}

#[test]
// Tests that `ptr_offset()` with 1 argument produces "ptr_offset() takes exactly 2 arguments".
fn test_error_ptr_offset_wrong_args() {
    expect_error(
        "<?php $p = ptr_null(); ptr_offset($p);",
        "ptr_offset() takes exactly 2 arguments",
    );
}

#[test]
// Tests that `ptr_offset()` with a non-pointer first argument produces "ptr_offset() requires a pointer argument".
fn test_error_ptr_offset_requires_pointer() {
    expect_error(
        "<?php ptr_offset(123, 8);",
        "ptr_offset() requires a pointer argument",
    );
}

#[test]
// Tests that `ptr_offset()` with a non-integer second argument produces "ptr_offset() second argument must be integer".
fn test_error_ptr_offset_requires_integer_offset() {
    expect_error(
        "<?php $p = ptr_null(); ptr_offset($p, \"8\");",
        "ptr_offset() second argument must be integer",
    );
}

#[test]
// Tests that `ptr_get()` with no arguments produces "ptr_get() takes exactly 1 argument".
fn test_error_ptr_get_wrong_args() {
    expect_error("<?php ptr_get();", "ptr_get() takes exactly 1 argument");
}

#[test]
// Tests that `ptr_get()` with a non-pointer argument produces "ptr_get() requires a pointer argument".
fn test_error_ptr_get_requires_pointer() {
    expect_error(
        "<?php ptr_get(123);",
        "ptr_get() requires a pointer argument",
    );
}

#[test]
// Tests that `ptr_read8()` with a non-pointer argument produces "ptr_read8() requires a pointer argument".
fn test_error_ptr_read8_requires_pointer() {
    expect_error(
        "<?php ptr_read8(123);",
        "ptr_read8() requires a pointer argument",
    );
}

#[test]
// Tests that `ptr_read16()` with no arguments produces "ptr_read16() takes exactly 1 argument".
fn test_error_ptr_read16_wrong_args() {
    expect_error(
        "<?php ptr_read16();",
        "ptr_read16() takes exactly 1 argument",
    );
}

#[test]
// Tests that `ptr_read16()` with a non-pointer argument produces "ptr_read16() requires a pointer argument".
fn test_error_ptr_read16_requires_pointer() {
    expect_error(
        "<?php ptr_read16(123);",
        "ptr_read16() requires a pointer argument",
    );
}

#[test]
// Tests that `ptr_read32()` with a non-pointer argument produces "ptr_read32() requires a pointer argument".
fn test_error_ptr_read32_requires_pointer() {
    expect_error(
        "<?php ptr_read32(123);",
        "ptr_read32() requires a pointer argument",
    );
}

#[test]
// Tests that `ptr_set()` with 1 argument produces "ptr_set() takes exactly 2 arguments".
fn test_error_ptr_set_wrong_args() {
    expect_error(
        "<?php ptr_set(ptr_null());",
        "ptr_set() takes exactly 2 arguments",
    );
}

#[test]
// Tests that `ptr_set()` with a non-pointer first argument produces "ptr_set() requires a pointer argument".
fn test_error_ptr_set_requires_pointer() {
    expect_error(
        "<?php ptr_set(123, 1);",
        "ptr_set() requires a pointer argument",
    );
}

#[test]
// Tests that `ptr_write8()` with a non-integer value argument produces "ptr_write8() value must be int".
fn test_error_ptr_write8_requires_int_value() {
    expect_error(
        "<?php $p = ptr_null(); ptr_write8($p, \"hello\");",
        "ptr_write8() value must be int",
    );
}

#[test]
// Tests that `ptr_write16()` with 1 argument produces "ptr_write16() takes exactly 2 arguments".
fn test_error_ptr_write16_wrong_args() {
    expect_error(
        "<?php $p = ptr_null(); ptr_write16($p);",
        "ptr_write16() takes exactly 2 arguments",
    );
}

#[test]
// Tests that `ptr_write16()` with a non-pointer first argument produces "ptr_write16() requires a pointer argument".
fn test_error_ptr_write16_requires_pointer() {
    expect_error(
        "<?php ptr_write16(123, 1);",
        "ptr_write16() requires a pointer argument",
    );
}

#[test]
// Tests that `ptr_write16()` with a non-integer value argument produces "ptr_write16() value must be int".
fn test_error_ptr_write16_requires_int_value() {
    expect_error(
        "<?php $p = ptr_null(); ptr_write16($p, \"hello\");",
        "ptr_write16() value must be int",
    );
}

#[test]
// Tests that `ptr_write32()` with a non-integer value argument produces "ptr_write32() value must be int".
fn test_error_ptr_write32_requires_int_value() {
    expect_error(
        "<?php $p = ptr_null(); ptr_write32($p, \"hello\");",
        "ptr_write32() value must be int",
    );
}

#[test]
// Tests that `ptr_read_string()` with 1 argument produces "ptr_read_string() takes exactly 2 arguments".
fn test_error_ptr_read_string_wrong_args() {
    expect_error(
        "<?php $p = ptr_null(); ptr_read_string($p);",
        "ptr_read_string() takes exactly 2 arguments",
    );
}

#[test]
// Tests that `ptr_read_string()` with a non-pointer first argument produces "ptr_read_string() requires a pointer argument".
fn test_error_ptr_read_string_requires_pointer() {
    expect_error(
        "<?php ptr_read_string(123, 4);",
        "ptr_read_string() requires a pointer argument",
    );
}

#[test]
// Tests that `ptr_read_string()` with a non-integer length argument produces "ptr_read_string() length must be int".
fn test_error_ptr_read_string_requires_int_length() {
    expect_error(
        "<?php $p = ptr_null(); ptr_read_string($p, \"4\");",
        "ptr_read_string() length must be int",
    );
}

#[test]
// Tests that `ptr_write_string()` with 1 argument produces "ptr_write_string() takes exactly 2 arguments".
fn test_error_ptr_write_string_wrong_args() {
    expect_error(
        "<?php $p = ptr_null(); ptr_write_string($p);",
        "ptr_write_string() takes exactly 2 arguments",
    );
}

#[test]
// Tests that `ptr_write_string()` with a non-pointer first argument produces "ptr_write_string() requires a pointer argument".
fn test_error_ptr_write_string_requires_pointer() {
    expect_error(
        "<?php ptr_write_string(123, \"hi\");",
        "ptr_write_string() requires a pointer argument",
    );
}

#[test]
// Tests that `ptr_write_string()` with a non-string value argument produces "ptr_write_string() string argument must be string".
fn test_error_ptr_write_string_requires_string_value() {
    expect_error(
        "<?php $p = ptr_null(); ptr_write_string($p, 123);",
        "ptr_write_string() string argument must be string",
    );
}

#[test]
// Tests that `ptr_sizeof()` with no arguments produces "ptr_sizeof() takes exactly 1 argument".
fn test_error_ptr_sizeof_wrong_args() {
    expect_error(
        "<?php ptr_sizeof();",
        "ptr_sizeof() takes exactly 1 argument",
    );
}

#[test]
// Tests that `ptr_sizeof()` with a non-literal (variable) argument produces "ptr_sizeof() argument must be a string literal".
fn test_error_ptr_sizeof_requires_literal() {
    expect_error(
        "<?php $t = \"int\"; ptr_sizeof($t);",
        "ptr_sizeof() argument must be a string literal",
    );
}

#[test]
// Tests that `ptr_sizeof()` with an unknown type name produces "Unknown type for ptr_sizeof(): NoSuchType".
fn test_error_ptr_sizeof_unknown_type() {
    expect_error(
        "<?php ptr_sizeof(\"NoSuchType\");",
        "Unknown type for ptr_sizeof(): NoSuchType",
    );
}

#[test]
// Tests that `ptr_cast<>` with no type name produces "Expected type name after 'ptr_cast<'".
fn test_error_ptr_cast_missing_type() {
    expect_error(
        "<?php ptr_cast<>(ptr_null());",
        "Expected type name after 'ptr_cast<'",
    );
}

#[test]
// Tests that `ptr_cast<int>` with a non-pointer argument produces "ptr_cast() requires a pointer argument".
fn test_error_ptr_cast_requires_pointer_argument() {
    expect_error(
        "<?php ptr_cast<int>(123);",
        "ptr_cast() requires a pointer argument",
    );
}

#[test]
// Tests that `ptr_cast<NoSuchType>` with an unknown target type produces "Unknown ptr_cast target type: NoSuchType".
fn test_error_ptr_cast_rejects_unknown_target() {
    expect_error(
        "<?php $p = ptr_null(); ptr_cast<NoSuchType>($p);",
        "Unknown ptr_cast target type: NoSuchType",
    );
}
