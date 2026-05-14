//! Purpose:
//! Integration or regression tests for SPL builtin diagnostics.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - These fixtures lock down conservative checker contracts that codegen can lower safely.

use super::*;

#[test]
fn test_error_spl_autoload_extensions_rejects_int_setter() {
    expect_error(
        "<?php spl_autoload_extensions(123);",
        "spl_autoload_extensions() argument must be a string literal or null",
    );
}

#[test]
fn test_error_spl_autoload_extensions_rejects_bool_setter() {
    expect_error(
        "<?php spl_autoload_extensions(true);",
        "spl_autoload_extensions() argument must be a string literal or null",
    );
}

#[test]
fn test_error_spl_autoload_extensions_rejects_array_setter() {
    expect_error(
        "<?php spl_autoload_extensions([\".php\"]);",
        "spl_autoload_extensions() argument must be a string literal or null",
    );
}

#[test]
fn test_error_spl_autoload_extensions_rejects_object_setter() {
    expect_error(
        "<?php class Box {} spl_autoload_extensions(new Box());",
        "spl_autoload_extensions() argument must be a string literal or null",
    );
}

#[test]
fn test_error_spl_autoload_extensions_rejects_dynamic_string_setter() {
    expect_error(
        "<?php $ext = \".php\"; spl_autoload_extensions($ext);",
        "spl_autoload_extensions() argument must be a string literal or null",
    );
}

#[test]
fn test_error_spl_object_id_rejects_mixed() {
    expect_error(
        "<?php function id(mixed $value): int { return spl_object_id($value); }",
        "spl_object_id() argument must be an object",
    );
}

#[test]
fn test_error_spl_object_hash_rejects_mixed() {
    expect_error(
        "<?php function hash_value(mixed $value): string { return spl_object_hash($value); }",
        "spl_object_hash() argument must be an object",
    );
}
