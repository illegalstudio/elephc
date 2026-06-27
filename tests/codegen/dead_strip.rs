//! Purpose:
//! Tests for runtime dead stripping — verifies that the linker eliminates
//! unused runtime helpers from the final binary and that programs using
//! specific runtime features keep the corresponding helpers.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Each test compiles a PHP program and asserts correct stdout, confirming
//!   that the dead-stripped binary still runs correctly.

use crate::support::*;

/// A hello-world program should still produce correct output after dead stripping.
#[test]
fn test_hello_world_after_dead_strip() {
    let out = compile_and_run(r#"<?php echo "hello\n";"#);
    assert_eq!(out, "hello\n");
}

/// A program that uses regex should keep preg_* helpers and run correctly.
#[test]
fn test_regex_program_after_dead_strip() {
    let out = compile_and_run(r#"<?php echo preg_match('/\d+/', "abc123") ? "match" : "no";"#);
    assert_eq!(out, "match");
}

/// A program that uses hash_init should keep hash helpers and run correctly.
#[test]
fn test_hash_program_after_dead_strip() {
    let out = compile_and_run(
        r#"<?php $ctx = hash_init("sha256"); hash_update($ctx, "hello"); echo hash_final($ctx);"#,
    );
    assert_eq!(
        out,
        "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
    );
}

/// A program that uses classes should keep object/vtable helpers and run correctly.
#[test]
fn test_class_program_after_dead_strip() {
    let out = compile_and_run(
        r#"<?php
class Foo { public int $x = 42; }
$f = new Foo();
echo $f->x;
"#,
    );
    assert_eq!(out, "42");
}

/// A program that uses fopen should keep I/O helpers and run correctly.
#[test]
fn test_fopen_program_after_dead_strip() {
    let out = compile_and_run(
        r#"<?php
$f = fopen("php://temp", "w+");
fwrite($f, "test");
fclose($f);
echo "ok";
"#,
    );
    assert_eq!(out, "ok");
}

/// A program that uses arrays and foreach should keep array helpers and run correctly.
#[test]
fn test_array_program_after_dead_strip() {
    let out = compile_and_run(
        r#"<?php
$arr = [1, 2, 3, 4, 5];
$sum = 0;
foreach ($arr as $v) { $sum += $v; }
echo $sum;
"#,
    );
    assert_eq!(out, "15");
}

/// A program that uses exceptions should keep exception helpers and run correctly.
#[test]
fn test_exception_program_after_dead_strip() {
    let out = compile_and_run(
        r#"<?php
try {
    throw new Exception("test");
} catch (Exception $e) {
    echo "caught: " . $e->getMessage();
}
"#,
    );
    assert_eq!(out, "caught: test");
}