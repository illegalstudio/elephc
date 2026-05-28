//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of I/O printing, including print basic, print integer, and print expression returns one.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

/// Verifies that `print` outputs a plain string literal unchanged.
#[test]
fn test_print_basic() {
    let out = compile_and_run("<?php print \"hello\";");
    assert_eq!(out, "hello");
}

/// Verifies that `print` outputs a bare integer literal as its decimal string representation.
#[test]
fn test_print_int() {
    let out = compile_and_run("<?php print 42;");
    assert_eq!(out, "42");
}

/// Verifies that `print` returns `1` when used in an expression context, matching PHP's value-for-side-effect semantics.
#[test]
fn test_print_expression_returns_one() {
    let out = compile_and_run("<?php $ok = print \"hello\"; echo \"\\n\"; echo $ok;");
    assert_eq!(out, "hello\n1");
}

/// Verifies that `print` returning `1` is correctly absorbed by `echo`, producing `"x1"` not `"x"` or a parse error.
#[test]
fn test_print_expression_can_be_nested_in_echo() {
    let out = compile_and_run("<?php echo print \"x\";");
    assert_eq!(out, "x1");
}

/// Verifies that `print` can accept a short-ternary expression as its operand; `print` binds tighter than `?:`, so `false ?: "fallback"` is evaluated first, then printed, and the resulting `1` return is echoed.
#[test]
fn test_print_expression_operand_accepts_short_ternary() {
    let out = compile_and_run("<?php echo print false ?: \"fallback\";");
    assert_eq!(out, "fallback1");
}

/// Verifies precedence: `print "x" and false` parses as `(print "x") and false` — `print` outputs and returns `1`, which is truthy, so `and false` does not suppress output.
#[test]
fn test_print_expression_binds_tighter_than_word_and() {
    let out = compile_and_run("<?php echo print \"x\" and false;");
    assert_eq!(out, "x");
}

/// Verifies that `print __FILE__` emits the source file path at compile time (magic constant lowering).
#[test]
fn test_print_expression_lowers_magic_constants() {
    let out = compile_and_run("<?php print __FILE__;");
    assert!(out.ends_with("test.php"), "unexpected __FILE__ output: {out}");
}

/// Verifies `var_dump` formats a bare integer as `int(N)` with a trailing newline.
#[test]
fn test_var_dump_int() {
    let out = compile_and_run("<?php var_dump(42);");
    assert_eq!(out, "int(42)\n");
}

/// Verifies `var_dump` formats a string as `string(N) "..."` including length, quotes, and a trailing newline.
#[test]
fn test_var_dump_string() {
    let out = compile_and_run(r#"<?php var_dump("hello");"#);
    assert_eq!(out, "string(5) \"hello\"\n");
}

/// Verifies `var_dump` formats boolean `true` as `bool(true)` with a trailing newline.
#[test]
fn test_var_dump_bool_true() {
    let out = compile_and_run("<?php var_dump(true);");
    assert_eq!(out, "bool(true)\n");
}

/// Verifies `var_dump` formats boolean `false` as `bool(false)` with a trailing newline.
#[test]
fn test_var_dump_bool_false() {
    let out = compile_and_run("<?php var_dump(false);");
    assert_eq!(out, "bool(false)\n");
}

/// Verifies `var_dump` formats `null` as `NULL` (uppercase, no parentheses) with a trailing newline.
#[test]
fn test_var_dump_null() {
    let out = compile_and_run("<?php var_dump(null);");
    assert_eq!(out, "NULL\n");
}

/// Verifies `var_dump` formats a float as `float(VALUE)` with full precision and a trailing newline.
#[test]
fn test_var_dump_float() {
    let out = compile_and_run("<?php var_dump(3.14);");
    assert_eq!(out, "float(3.14)\n");
}

/// Verifies `var_dump` emits the correct concrete type tag and value for each heterogeneous assoc-array slot: int, string, bool, null, array, and object.
#[test]
fn test_var_dump_mixed_prints_concrete_payload() {
    let out = compile_and_run(
        r#"<?php
class Box {}

$map = [
    "i" => 42,
    "s" => "hello",
    "b" => true,
    "n" => null,
    "a" => [1, 2],
    "o" => new Box(),
];

var_dump($map["i"]);
var_dump($map["s"]);
var_dump($map["b"]);
var_dump($map["n"]);
var_dump($map["a"]);
var_dump($map["o"]);
"#,
    );
    assert_eq!(
        out,
        "int(42)\nstring(5) \"hello\"\nbool(true)\nNULL\narray(2) {\n}\nobject(Box)\n"
    );
}

/// Verifies `print_r` outputs a bare integer as its decimal string representation (no type label), no trailing newline.
#[test]
fn test_print_r_int() {
    let out = compile_and_run("<?php print_r(42);");
    assert_eq!(out, "42");
}

/// Verifies `print_r` outputs a string unchanged, no type label, no trailing newline.
#[test]
fn test_print_r_string() {
    let out = compile_and_run(r#"<?php print_r("hello");"#);
    assert_eq!(out, "hello");
}

/// Verifies `print_r` outputs `1` for boolean `true`, no type label, no trailing newline.
#[test]
fn test_print_r_bool_true() {
    let out = compile_and_run("<?php print_r(true);");
    assert_eq!(out, "1");
}

/// Verifies `print_r` outputs an empty string for boolean `false`.
#[test]
fn test_print_r_bool_false() {
    let out = compile_and_run("<?php print_r(false);");
    assert_eq!(out, "");
}

/// Verifies `print_r` outputs `Array\n` for a non-empty indexed array, showing only the array header (struct dump not yet implemented).
#[test]
fn test_print_r_array() {
    let out = compile_and_run("<?php print_r([1, 2, 3]);");
    assert_eq!(out, "Array\n");
}

/// Verifies `var_dump` formats each argument independently with correct type tags and a trailing newline per call, in source order.
#[test]
fn test_var_dump_multiple() {
    let out = compile_and_run(
        r#"<?php
var_dump(1);
var_dump("hi");
var_dump(true);
"#,
    );
    assert_eq!(out, "int(1)\nstring(2) \"hi\"\nbool(true)\n");
}

// --- File I/O: CSV, timestamps, directory listing, temp files, seek/rewind/eof ---
