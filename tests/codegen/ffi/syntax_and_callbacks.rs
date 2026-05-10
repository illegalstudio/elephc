//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of FFI syntax and callbacks, including FFI extern block syntax, FFI extern lib function syntax, and FFI extern void return.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

#[test]
fn test_ffi_extern_block_syntax() {
    let out = compile_and_run(
        r#"<?php
extern "System" {
    function abs(int $n): int;
    function atoi(string $s): int;
}
echo abs(-7) . "," . atoi("99");
"#,
    );
    assert_eq!(out, "7,99");
}

#[test]
fn test_ffi_extern_lib_function_syntax() {
    let out = compile_and_run(
        r#"<?php
extern "System" function abs(int $n): int;
echo abs(-1);
"#,
    );
    assert_eq!(out, "1");
}

#[test]
fn test_ffi_extern_void_return() {
    let out = compile_and_run(
        r#"<?php
extern function abs(int $n): int;
$x = abs(-5);
echo $x;
"#,
    );
    assert_eq!(out, "5");
}

#[test]
fn test_ffi_extern_float_arg_and_return() {
    let out = compile_and_run(
        r#"<?php
extern function sqrt(float $x): float;
echo sqrt(144.0);
"#,
    );
    assert_eq!(out, "12");
}

#[test]
fn test_ffi_extern_multiple_args() {
    let out = compile_and_run(
        r#"<?php
extern function strtol(string $s, ptr $endptr, int $base): int;
echo strtol("FF", ptr_null(), 16);
"#,
    );
    assert_eq!(out, "255");
}

#[test]
fn test_ffi_extern_multiple_string_args() {
    let out = compile_and_run(
        r#"<?php
extern function strcmp(string $left, string $right): int;
echo strcmp("aa", "ab") < 0 ? "ok" : "bad";
"#,
    );
    assert_eq!(out, "ok");
}

#[test]
fn test_ffi_extern_multiple_string_args_free_all_borrowed_cstr_temps() {
    let baseline = compile_and_run_with_gc_stats(
        r#"<?php
extern function strcmp(string $left, string $right): int;
"#,
    );
    assert!(
        baseline.success,
        "baseline program failed: {}",
        baseline.stderr
    );
    let out = compile_and_run_with_gc_stats(
        r#"<?php
extern function strcmp(string $left, string $right): int;
strcmp("aa", "ab");
strcmp("bb", "bb");
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

#[test]
fn test_ffi_extern_global() {
    let out = compile_and_run(
        r#"<?php
extern global ptr $environ;
echo ptr_is_null($environ) ? "fail" : "ok";
"#,
    );
    assert_eq!(out, "ok");
}

#[test]
fn test_ffi_callback_signal_handler() {
    let out = compile_and_run(
        r#"<?php
extern function signal(int $sig, callable $handler): ptr;
extern function raise(int $sig): int;

function on_signal($sig) {
    echo $sig;
}

signal(15, "on_signal");
raise(15);
"#,
    );
    assert_eq!(out, "15");
}

#[test]
fn test_ffi_extern_non_string_global_smoke() {
    let out = compile_and_run(
        r#"<?php
extern function getpid(): int;
$pid = getpid();
echo $pid > 0 ? "ok" : "fail";
"#,
    );
    assert_eq!(out, "ok");
}

#[test]
fn test_ffi_extern_in_function() {
    let out = compile_and_run(
        r#"<?php
extern function abs(int $n): int;
function my_abs($x) {
    return abs($x);
}
echo my_abs(-10);
"#,
    );
    assert_eq!(out, "10");
}
