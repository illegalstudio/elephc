//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of strings interpolation and hashes, including interpolation simple, interpolation multiple, and interpolation at start.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

/// Verifies simple double-quoted string interpolation with one variable.
/// Fixture: assign a string to `$name`, then echo `"Hello $name"`.
#[test]
fn test_string_interpolation_simple() {
    let out = compile_and_run(r#"<?php $name = "World"; echo "Hello $name";"#);
    assert_eq!(out, "Hello World");
}

/// Verifies double-quoted string interpolation with two variables adjacent in the string.
/// Fixture: `$a = "foo"`, `$b = "bar"`, then echo `"$a and $b"`.
#[test]
fn test_string_interpolation_multiple() {
    let out = compile_and_run(r#"<?php $a = "foo"; $b = "bar"; echo "$a and $b";"#);
    assert_eq!(out, "foo and bar");
}

/// Verifies double-quoted string interpolation when the variable appears at the start of the string.
/// Fixture: `$x = "hi"`, then echo `"$x there"`.
#[test]
fn test_string_interpolation_at_start() {
    let out = compile_and_run(r#"<?php $x = "hi"; echo "$x there";"#);
    assert_eq!(out, "hi there");
}

/// Verifies double-quoted string interpolation when the variable appears at the end of the string.
/// Fixture: `$x = "world"`, then echo `"hello $x"`.
#[test]
fn test_string_interpolation_at_end() {
    let out = compile_and_run(r#"<?php $x = "world"; echo "hello $x";"#);
    assert_eq!(out, "hello world");
}

/// Verifies that single-quoted strings do NOT perform variable interpolation.
/// Fixture: `$x = 42`, then echo `'$x'`; expects literal "$x" in output.
#[test]
fn test_string_no_interpolation() {
    // Single-quoted strings should NOT interpolate
    let out = compile_and_run("<?php $x = 42; echo '$x';");
    assert_eq!(out, "$x");
}

/// Verifies `md5()` produces the correct hash for an empty string input.
#[test]
fn test_md5_empty() {
    let out = compile_and_run(r#"<?php echo md5("");"#);
    assert_eq!(out, "d41d8cd98f00b204e9800998ecf8427e");
}

/// Verifies `md5()` produces the correct hash for "Hello".
#[test]
fn test_md5_hello() {
    let out = compile_and_run(r#"<?php echo md5("Hello");"#);
    assert_eq!(out, "8b1a9953c4611296a827abf8c47804d7");
}

/// Verifies `sha1()` produces the correct hash for an empty string input.
#[test]
fn test_sha1_empty() {
    let out = compile_and_run(r#"<?php echo sha1("");"#);
    assert_eq!(out, "da39a3ee5e6b4b0d3255bfef95601890afd80709");
}

/// Verifies `sha1()` produces the correct hash for "Hello".
#[test]
fn test_sha1_hello() {
    let out = compile_and_run(r#"<?php echo sha1("Hello");"#);
    assert_eq!(out, "f7ff9e8b7bb2e09b70935a5d785e0cc5d9d0abf0");
}

// --- hash() ---

/// Verifies `hash("md5", ...)` produces the correct hash for "Hello".
#[test]
fn test_hash_md5() {
    let out = compile_and_run(r#"<?php echo hash("md5", "Hello");"#);
    assert_eq!(out, "8b1a9953c4611296a827abf8c47804d7");
}

/// Verifies `hash("sha1", ...)` produces the correct hash for "Hello".
#[test]
fn test_hash_sha1() {
    let out = compile_and_run(r#"<?php echo hash("sha1", "Hello");"#);
    assert_eq!(out, "f7ff9e8b7bb2e09b70935a5d785e0cc5d9d0abf0");
}

/// Verifies `hash("sha256", ...)` produces the correct hash for "Hello".
#[test]
fn test_hash_sha256() {
    let out = compile_and_run(r#"<?php echo hash("sha256", "Hello");"#);
    assert_eq!(
        out,
        "185f8db32271fe25f561a6fc938b2e264306ec304eda518007d1764826381969"
    );
}

// --- sscanf() ---
