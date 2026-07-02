//! Purpose:
//! Regression tests for issue #424: array/hash strict equality (===) was
//! unsupported by the EIR backend, causing a compile error for any array
//! or associative-array operand to `===` / `!==`.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Strict equality compares same key/value pairs in the same order with
//!   identical types (no juggling). Loose equality (`==`) is deferred.

use crate::support::compile_and_run;

#[test]
fn test_indexed_array_strict_eq_true() {
    let out = compile_and_run(r#"<?php
var_dump([1, 2, 3] === [1, 2, 3]);
"#);
    assert_eq!(out, "bool(true)\n");
}

#[test]
fn test_indexed_array_strict_eq_false_different_values() {
    let out = compile_and_run(r#"<?php
var_dump([1, 2, 3] === [1, 2, 4]);
"#);
    assert_eq!(out, "bool(false)\n");
}

#[test]
fn test_indexed_array_strict_eq_false_different_length() {
    let out = compile_and_run(r#"<?php
var_dump([1, 2, 3] === [1, 2, 3, 4]);
"#);
    assert_eq!(out, "bool(false)\n");
}

#[test]
fn test_empty_array_strict_eq() {
    let out = compile_and_run(r#"<?php
var_dump([] === []);
"#);
    assert_eq!(out, "bool(true)\n");
}

#[test]
fn test_indexed_array_strict_not_eq() {
    let out = compile_and_run(r#"<?php
var_dump([1, 2, 3] !== [1, 2, 4]);
"#);
    assert_eq!(out, "bool(true)\n");
}

#[test]
fn test_string_array_strict_eq() {
    let out = compile_and_run(r#"<?php
var_dump(["a", "b", "c"] === ["a", "b", "c"]);
"#);
    assert_eq!(out, "bool(true)\n");
}

#[test]
fn test_string_array_strict_eq_false() {
    let out = compile_and_run(r#"<?php
var_dump(["a", "b"] === ["a", "c"]);
"#);
    assert_eq!(out, "bool(false)\n");
}

#[test]
fn test_array_alias_strict_eq() {
    let out = compile_and_run(r#"<?php
$a = [1, 2, 3];
$b = $a;
var_dump($a === $b);
"#);
    assert_eq!(out, "bool(true)\n");
}

#[test]
fn test_hash_strict_eq_true() {
    let out = compile_and_run(r#"<?php
var_dump(["a" => 1, "b" => 2] === ["a" => 1, "b" => 2]);
"#);
    assert_eq!(out, "bool(true)\n");
}

#[test]
fn test_hash_strict_eq_false_different_order() {
    let out = compile_and_run(r#"<?php
var_dump(["a" => 1, "b" => 2] === ["b" => 2, "a" => 1]);
"#);
    assert_eq!(out, "bool(false)\n");
}

#[test]
fn test_hash_strict_eq_string_values_true() {
    let out = compile_and_run(r#"<?php
var_dump(["x" => "hello"] === ["x" => "hello"]);
"#);
    assert_eq!(out, "bool(true)\n");
}

#[test]
fn test_hash_strict_eq_string_values_false() {
    let out = compile_and_run(r#"<?php
var_dump(["x" => "hello"] === ["x" => "world"]);
"#);
    assert_eq!(out, "bool(false)\n");
}

#[test]
fn test_hash_strict_not_eq() {
    let out = compile_and_run(r#"<?php
var_dump(["a" => 1] !== ["a" => 2]);
"#);
    assert_eq!(out, "bool(true)\n");
}

#[test]
fn test_empty_hash_strict_eq() {
    let out = compile_and_run(r#"<?php
var_dump(["a" => 1, "b" => 2] === ["a" => 1, "b" => 2, "c" => 3]);
"#);
    assert_eq!(out, "bool(false)\n");
}