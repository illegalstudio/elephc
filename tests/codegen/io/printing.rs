//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of I/O printing, including print basic, print integer, and print expression returns one.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

#[test]
fn test_print_basic() {
    let out = compile_and_run("<?php print \"hello\";");
    assert_eq!(out, "hello");
}

#[test]
fn test_print_int() {
    let out = compile_and_run("<?php print 42;");
    assert_eq!(out, "42");
}

#[test]
fn test_print_expression_returns_one() {
    let out = compile_and_run("<?php $ok = print \"hello\"; echo \"\\n\"; echo $ok;");
    assert_eq!(out, "hello\n1");
}

#[test]
fn test_print_expression_can_be_nested_in_echo() {
    let out = compile_and_run("<?php echo print \"x\";");
    assert_eq!(out, "x1");
}

#[test]
fn test_print_expression_operand_accepts_short_ternary() {
    let out = compile_and_run("<?php echo print false ?: \"fallback\";");
    assert_eq!(out, "fallback1");
}

#[test]
fn test_print_expression_binds_tighter_than_word_and() {
    let out = compile_and_run("<?php echo print \"x\" and false;");
    assert_eq!(out, "x");
}

#[test]
fn test_print_expression_lowers_magic_constants() {
    let out = compile_and_run("<?php print __FILE__;");
    assert!(out.ends_with("test.php"), "unexpected __FILE__ output: {out}");
}

#[test]
fn test_var_dump_int() {
    let out = compile_and_run("<?php var_dump(42);");
    assert_eq!(out, "int(42)\n");
}

#[test]
fn test_var_dump_string() {
    let out = compile_and_run(r#"<?php var_dump("hello");"#);
    assert_eq!(out, "string(5) \"hello\"\n");
}

#[test]
fn test_var_dump_bool_true() {
    let out = compile_and_run("<?php var_dump(true);");
    assert_eq!(out, "bool(true)\n");
}

#[test]
fn test_var_dump_bool_false() {
    let out = compile_and_run("<?php var_dump(false);");
    assert_eq!(out, "bool(false)\n");
}

#[test]
fn test_var_dump_null() {
    let out = compile_and_run("<?php var_dump(null);");
    assert_eq!(out, "NULL\n");
}

#[test]
fn test_var_dump_float() {
    let out = compile_and_run("<?php var_dump(3.14);");
    assert_eq!(out, "float(3.14)\n");
}

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

#[test]
fn test_print_r_int() {
    let out = compile_and_run("<?php print_r(42);");
    assert_eq!(out, "42");
}

#[test]
fn test_print_r_string() {
    let out = compile_and_run(r#"<?php print_r("hello");"#);
    assert_eq!(out, "hello");
}

#[test]
fn test_print_r_bool_true() {
    let out = compile_and_run("<?php print_r(true);");
    assert_eq!(out, "1");
}

#[test]
fn test_print_r_bool_false() {
    let out = compile_and_run("<?php print_r(false);");
    assert_eq!(out, "");
}

#[test]
fn test_print_r_array() {
    let out = compile_and_run("<?php print_r([1, 2, 3]);");
    assert_eq!(out, "Array\n");
}

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
