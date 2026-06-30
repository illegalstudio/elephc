//! Purpose:
//! Regression tests for issue #425: string-key reads on foreach-rebuilt
//! Array(Mixed) results were rejected at compile time.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - After foreach rebuilds an array as Array(Mixed), string-key reads must
//!   work and return the correct values.

use crate::support::compile_and_run;

#[test]
fn test_foreach_mixed_string_key_direct_read() {
    let out = compile_and_run(r#"<?php
function rebuild(array $src): array {
    $dst = [];
    foreach ($src as $k => $v) { $dst[$k] = $v; }
    return $dst;
}
$r = rebuild(["name" => "Alice", "age" => 30]);
echo $r["name"], $r["age"];
"#);
    assert_eq!(out, "Alice30");
}

#[test]
fn test_foreach_mixed_int_key_direct_read() {
    let out = compile_and_run(r#"<?php
function rebuild(array $src): array {
    $dst = [];
    foreach ($src as $k => $v) { $dst[$k] = $v; }
    return $dst;
}
$r = rebuild([10, 20, 30]);
echo $r[0], $r[1], $r[2];
"#);
    assert_eq!(out, "102030");
}

#[test]
fn test_foreach_mixed_missing_key_returns_null() {
    let out = compile_and_run(r#"<?php
function rebuild(array $src): array {
    $dst = [];
    foreach ($src as $k => $v) { $dst[$k] = $v; }
    return $dst;
}
$r = rebuild(["a" => 1]);
var_dump($r["missing"]);
"#);
    assert_eq!(out, "NULL\n");
}