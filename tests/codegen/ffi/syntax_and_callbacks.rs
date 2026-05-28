//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of FFI syntax and callbacks, including FFI extern block syntax, FFI extern lib function syntax, and FFI extern void return.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

// Verifies extern block declares multiple FFI extern functions and they are callable from PHP.
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

// Verifies single-line `extern "System" function` syntax declares an FFI extern callable from PHP.
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

// Verifies FFI extern function with `void` return type and `int` argument is compiled correctly.
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

// Verifies FFI extern function with `float` argument and `float` return type is compiled correctly.
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

// Verifies FFI extern function with `ptr` and `int` mixed arguments is compiled correctly.
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

// Verifies FFI extern function with two `string` arguments is compiled correctly.
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

// Verifies that borrowed C-string temporaries from two FFI extern calls with `string` args are freed without leaking.
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

// Verifies `extern global ptr $environ` declares an FFI external global pointer variable.
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

// Verifies FFI extern `signal` and `raise` are callable and a PHP function can be used as a signal handler.
/// Verifies extern callback string literals resolve function names case-insensitively.
#[test]
fn test_ffi_callback_signal_handler() {
    let out = compile_and_run(
        r#"<?php
extern function signal(int $sig, callable $handler): ptr;
extern function raise(int $sig): int;

function on_signal($sig) {
    echo $sig;
}

signal(15, "ON_SIGNAL");
raise(15);
"#,
    );
    assert_eq!(out, "15");
}

/// Verifies FFI `callable` parameters accept environment-free first-class callable descriptors.
#[test]
fn test_ffi_callback_signal_handler_first_class_callable() {
    let out = compile_and_run(
        r#"<?php
extern function signal(int $sig, callable $handler): ptr;
extern function raise(int $sig): int;

function on_signal(int $sig): void {
    echo $sig + 1;
}

$handler = on_signal(...);
signal(15, $handler);
raise(15);
"#,
    );
    assert_eq!(out, "16");
}

/// Verifies FFI `callable` parameters accept closure descriptors without captures.
#[test]
fn test_ffi_callback_signal_handler_closure_descriptor() {
    let out = compile_and_run(
        r#"<?php
extern function signal(int $sig, callable $handler): ptr;
extern function raise(int $sig): int;

$handler = function(int $sig): void {
    echo $sig + 2;
};

signal(15, $handler);
raise(15);
"#,
    );
    assert_eq!(out, "17");
}

/// Verifies FFI `callable` parameters preserve closure capture environments through trampolines.
#[test]
fn test_ffi_callback_signal_handler_closure_capture_descriptor() {
    let out = compile_and_run(
        r#"<?php
extern function signal(int $sig, callable $handler): ptr;
extern function raise(int $sig): int;

$delta = 3;
$handler = function(int $sig) use ($delta): void {
    echo $sig + $delta;
};

signal(15, $handler);
raise(15);
"#,
    );
    assert_eq!(out, "18");
}

/// Verifies FFI callback trampolines preserve first-class method receiver environments.
#[test]
fn test_ffi_callback_signal_handler_first_class_method_receiver_descriptor() {
    let out = compile_and_run(
        r#"<?php
extern function signal(int $sig, callable $handler): ptr;
extern function raise(int $sig): int;

class Handler {
    public int $delta;

    public function __construct(int $delta) {
        $this->delta = $delta;
    }

    public function onSignal(int $sig): void {
        echo $sig + $this->delta;
    }
}

$handler = (new Handler(4))->onSignal(...);
signal(15, $handler);
raise(15);
"#,
    );
    assert_eq!(out, "19");
}

/// Verifies FFI callback trampolines preserve branch-selected descriptor environments.
#[test]
fn test_ffi_callback_signal_handler_branch_selected_descriptor() {
    let out = compile_and_run(
        r#"<?php
extern function signal(int $sig, callable $handler): ptr;
extern function raise(int $sig): int;

$delta = 5;
$handler = true
    ? function(int $sig) use ($delta): void { echo $sig + $delta; }
    : function(int $sig): void { echo $sig; };

signal(15, $handler);
raise(15);
"#,
    );
    assert_eq!(out, "20");
}

// Smoke test: verifies non-string FFI extern function returning `int` is callable and returns a positive pid.
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

// Verifies FFI extern function declared inside a PHP function scope is callable and resolves correctly.
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
