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

/// Verifies complex `{$var}` interpolation: the braces delimit the variable and are not
/// emitted literally.
#[test]
fn test_string_interpolation_complex_simple_var() {
    let out = compile_and_run(r#"<?php $b = "B"; echo "a{$b}c";"#);
    assert_eq!(out, "aBc");
}

/// Verifies complex `{$arr[idx]}` interpolation evaluates the array access inside braces.
#[test]
fn test_string_interpolation_complex_array_access() {
    let out = compile_and_run(r#"<?php $a = [1, 2, 3]; echo "x{$a[1]}y";"#);
    assert_eq!(out, "x2y");
}

/// Verifies complex `{$obj->prop}` interpolation evaluates the property access inside braces.
#[test]
fn test_string_interpolation_complex_property() {
    let out = compile_and_run(r#"<?php class C { public $x = 5; } $o = new C(); echo "{$o->x}";"#);
    assert_eq!(out, "5");
}

/// Verifies simple `$arr[key]` interpolation with a bareword key (treated as a string key).
#[test]
fn test_string_interpolation_simple_array_bareword() {
    let out = compile_and_run(r#"<?php $a = ["k" => "V"]; echo "X $a[k] Y";"#);
    assert_eq!(out, "X V Y");
}

/// Verifies simple `$arr[int]` interpolation with an integer key.
#[test]
fn test_string_interpolation_simple_array_int() {
    let out = compile_and_run(r#"<?php $a = [10, 20]; echo "$a[1]";"#);
    assert_eq!(out, "20");
}

/// Verifies simple `$obj->prop` interpolation reads a single property.
#[test]
fn test_string_interpolation_simple_property() {
    let out =
        compile_and_run(r#"<?php class C { public $x = 5; } $o = new C(); echo "v=$o->x";"#);
    assert_eq!(out, "v=5");
}

/// Verifies a `{` not followed by `$` stays a literal brace (PHP only treats `{$` as the
/// start of complex interpolation).
#[test]
fn test_string_literal_brace_not_interpolation() {
    let out = compile_and_run(r#"<?php echo "a{b}c";"#);
    assert_eq!(out, "a{b}c");
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

// --- crc32() ---

// Verifies crc32() against PHP reference vectors, including the empty string (0)
// and the canonical "123456789" CRC-32 test vector. The result is a non-negative
// 64-bit int (the unsigned 32-bit checksum), matching 64-bit PHP.
/// Verifies compiled PHP output for crc32 known vectors.
#[test]
fn test_crc32_known_vectors() {
    let out = compile_and_run(
        r#"<?php echo crc32("") . "|" . crc32("123456789") . "|" . crc32("The quick brown fox");"#,
    );
    assert_eq!(out, "0|3421780262|3074782430");
}

// Verifies crc32() resolves through PHP's case-insensitive builtin lookup and
// that its result feeds arithmetic as a plain int.
/// Verifies compiled PHP output for crc32 case insensitive and int.
#[test]
fn test_crc32_case_insensitive_and_int() {
    let out = compile_and_run(r#"<?php echo CRC32("abc") + 1;"#);
    assert_eq!(out, "891568579"); // crc32("abc") = 891568578
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
