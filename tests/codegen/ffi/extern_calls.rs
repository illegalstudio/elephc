//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of FFI extern calls, including FFI extern abs, FFI extern atoi, and FFI extern strlen.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

#[test]
fn test_ffi_extern_abs() {
    let out = compile_and_run(
        r#"<?php
extern function abs(int $n): int;
echo abs(-42);
"#,
    );
    assert_eq!(out, "42");
}

#[test]
fn test_ffi_extern_atoi() {
    let out = compile_and_run(
        r#"<?php
extern function atoi(string $s): int;
echo atoi("12345");
"#,
    );
    assert_eq!(out, "12345");
}

#[test]
fn test_ffi_extern_strlen() {
    let out = compile_and_run(
        r#"<?php
extern function strlen(string $s): int;
echo strlen("hello world");
"#,
    );
    assert_eq!(out, "11");
}

#[test]
fn test_ffi_extern_named_arguments_reorder_call() {
    let out = compile_and_run(
        r#"<?php
extern function strcmp(string $left, string $right): int;
echo strcmp(right: "b", left: "a") < 0 ? "lt" : "no";
"#,
    );
    assert_eq!(out, "lt");
}

#[test]
fn test_ffi_extern_named_arguments_after_spread() {
    let out = compile_and_run(
        r#"<?php
extern function strcmp(string $left, string $right): int;
$args = ["a"];
echo strcmp(...$args, right: "b") < 0 ? "lt" : "no";
"#,
    );
    assert_eq!(out, "lt");
}

#[test]
fn test_ffi_extern_named_arguments_preserve_source_evaluation_order() {
    let out = compile_and_run(
        r#"<?php
extern function strcmp(string $left, string $right): int;
function left_arg() {
    echo "l";
    return "a";
}
function right_arg() {
    echo "r";
    return "b";
}
echo ":" . (strcmp(right: right_arg(), left: left_arg()) < 0 ? "lt" : "no");
"#,
    );
    assert_eq!(out, "rl:lt");
}

#[test]
fn test_ffi_extern_positional_arguments_preserve_source_evaluation_order() {
    let out = compile_and_run(
        r#"<?php
extern function strcmp(string $left, string $right): int;
function left_arg() {
    echo "L";
    return "a";
}
function right_arg() {
    echo "R";
    return "b";
}
echo ":" . (strcmp(left_arg(), right_arg()) < 0 ? "lt" : "ge");
"#,
    );
    assert_eq!(out, "LR:lt");
}

#[test]
fn test_ffi_extern_named_arguments_after_spread_evaluate_spread_once() {
    let out = compile_and_run(
        r#"<?php
extern function strcmp(string $left, string $right): int;
function args() {
    echo "x";
    return ["a"];
}
function right_arg() {
    echo "r";
    return "b";
}
echo ":" . (strcmp(...args(), right: right_arg()) < 0 ? "lt" : "no");
"#,
    );
    assert_eq!(out, "xr:lt");
}

#[test]
fn test_ffi_extern_assoc_spread_literal_maps_to_named_args() {
    let out = compile_and_run(
        r#"<?php
extern function strcmp(string $left, string $right): int;
echo strcmp(...["right" => "b", "left" => "a"]) < 0 ? "lt" : "no";
"#,
    );
    assert_eq!(out, "lt");
}

#[test]
fn test_ffi_extern_call_in_concat_restores_concat_cursor() {
    let out = compile_and_run(
        r#"<?php
extern function strlen(string $s): int;
echo "len=" . strlen("hello");
"#,
    );
    assert_eq!(out, "len=5");
}
#[test]
fn test_ffi_extern_strlen_frees_borrowed_cstr_temp() {
    let baseline = compile_and_run_with_gc_stats(
        r#"<?php
extern function strlen(string $s): int;
"#,
    );
    assert!(
        baseline.success,
        "baseline program failed: {}",
        baseline.stderr
    );
    let out = compile_and_run_with_gc_stats(
        r#"<?php
extern function strlen(string $s): int;
strlen("hello");
strlen("world");
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    let (baseline_allocs, baseline_frees) = parse_gc_stats(&baseline.stderr);
    let (allocs, frees) = parse_gc_stats(&out.stderr);
    assert_eq!(
        allocs - baseline_allocs,
        frees - baseline_frees,
        "{}",
        out.stderr
    );
}
