//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of FFI extern calls, including FFI extern abs, FFI extern atoi, and FFI extern strlen.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

/// Verifies FFI extern call to libc `abs` with a negative integer argument.
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

/// Verifies FFI extern call to libc `atoi` with a string argument, returning integer 12345.
#[test]
fn test_ffi_extern_atoi() {
    // Verifies FFI extern call to libc `atoi` with a string argument, returning integer 12345.
    let out = compile_and_run(
        r#"<?php
extern function atoi(string $s): int;
echo atoi("12345");
"#,
    );
    assert_eq!(out, "12345");
}

/// Verifies FFI extern call to libc `strlen` on a static string, returning 11.
#[test]
fn test_ffi_extern_strlen() {
    // Verifies FFI extern call to libc `strlen` on a static string, returning 11.
    let out = compile_and_run(
        r#"<?php
extern function strlen(string $s): int;
echo strlen("hello world");
"#,
    );
    assert_eq!(out, "11");
}

/// Verifies `call_user_func("STRLEN", ...)` resolves to FFI extern `strlen` via case-insensitive builtin lookup.
#[test]
fn test_ffi_extern_call_user_func_string_callback() {
    // Verifies `call_user_func("STRLEN", ...)` resolves to FFI extern `strlen` via case-insensitive builtin lookup.
    let out = compile_and_run(
        r#"<?php
extern function strlen(string $s): int;
echo call_user_func("STRLEN", "hello");
"#,
    );
    assert_eq!(out, "5");
}

/// Verifies `call_user_func_array("STRLEN", ["hello"])` passes spread array as single argument to FFI extern `strlen`.
#[test]
fn test_ffi_extern_call_user_func_array_string_callback() {
    // Verifies `call_user_func_array("STRLEN", ["hello"])` passes spread array as single argument to FFI extern `strlen`.
    let out = compile_and_run(
        r#"<?php
extern function strlen(string $s): int;
echo call_user_func_array("STRLEN", ["hello"]);
"#,
    );
    assert_eq!(out, "5");
}

/// Verifies dynamic extern string callbacks use runtime descriptor invokers.
#[test]
fn test_ffi_extern_dynamic_call_user_func_array_uses_descriptor_invoker() {
    let source = r#"<?php
extern function atoi(string $s): int;
$callback = "ATOI";
$args = ["1234"];
echo call_user_func_array($callback, $args) + 8;
"#;
    let out = compile_and_run(source);
    assert_eq!(out, "1242");

    let dir = make_cli_test_dir("elephc_extern_runtime_callable_descriptor");
    let (user_asm, _runtime_asm, _required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    assert!(
        user_asm.contains("callable_extern") && user_asm.contains("callable_invoker"),
        "dynamic extern callbacks should route through generated descriptor invokers:\n{}",
        user_asm
    );
    let _ = fs::remove_dir_all(dir);
}

/// Verifies dynamic `call_user_func()` can dispatch extern callables by runtime string name.
#[test]
fn test_ffi_extern_dynamic_call_user_func_string_callback() {
    let out = compile_and_run(
        r#"<?php
extern function atoi(string $s): int;
$callback = "atoi";
echo call_user_func($callback, "37") + 5;
"#,
    );
    assert_eq!(out, "42");
}

/// Verifies named arguments (`left:`, `right:`) are correctly reordered to match FFI extern parameter order.
#[test]
fn test_ffi_extern_named_arguments_reorder_call() {
    // Verifies named arguments (`left:`, `right:`) are correctly reordered to match FFI extern parameter order.
    let out = compile_and_run(
        r#"<?php
extern function strcmp(string $left, string $right): int;
echo strcmp(right: "b", left: "a") < 0 ? "lt" : "no";
"#,
    );
    assert_eq!(out, "lt");
}

/// Verifies named arguments after a positional spread are handled correctly for FFI extern calls.
#[test]
fn test_ffi_extern_named_arguments_after_spread() {
    // Verifies named arguments after a positional spread are handled correctly for FFI extern calls.
    let out = compile_and_run(
        r#"<?php
extern function strcmp(string $left, string $right): int;
$args = ["a"];
echo strcmp(...$args, right: "b") < 0 ? "lt" : "no";
"#,
    );
    assert_eq!(out, "lt");
}

/// Verifies named arguments evaluate source expressions left-to-right and that the callee receives them in ABI order.
/// Output "rl:lt" confirms `right_arg()` runs before `left_arg()` (named `right:` appears before `left:` in source).
#[test]
fn test_ffi_extern_named_arguments_preserve_source_evaluation_order() {
    // Verifies named arguments evaluate source expressions left-to-right and that the callee receives them in ABI order.
    // Output "rl:lt" confirms `right_arg()` runs before `left_arg()` (named `right:` appears before `left:` in source).
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

/// Verifies positional arguments evaluate source expressions left-to-right; output "LR:lt" confirms `left_arg()` runs first.
#[test]
fn test_ffi_extern_positional_arguments_preserve_source_evaluation_order() {
    // Verifies positional arguments evaluate source expressions left-to-right; output "LR:lt" confirms `left_arg()` runs first.
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

/// Verifies the spread argument `...args()` is evaluated exactly once when named arguments follow it.
/// Output "xr:lt" confirms `args()` prints "x" once and `right_arg()` prints "r".
#[test]
fn test_ffi_extern_named_arguments_after_spread_evaluate_spread_once() {
    // Verifies the spread argument `...args()` is evaluated exactly once when named arguments follow it.
    // Output "xr:lt" confirms `args()` prints "x" once and `right_arg()` prints "r".
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

/// Verifies a static assoc-array spread literal maps string keys to named arguments for FFI extern calls.
/// `...["right" => "b", "left" => "a"]` behaves like `left: "a", right: "b"` and compares as expected ("lt").
#[test]
fn test_ffi_extern_assoc_spread_literal_maps_to_named_args() {
    // Verifies a static assoc-array spread literal maps string keys to named arguments for FFI extern calls.
    // `...["right" => "b", "left" => "a"]` behaves like `left: "a", right: "b"` and compares as expected ("lt").
    let out = compile_and_run(
        r#"<?php
extern function strcmp(string $left, string $right): int;
echo strcmp(...["right" => "b", "left" => "a"]) < 0 ? "lt" : "no";
"#,
    );
    assert_eq!(out, "lt");
}

/// Verifies FFI extern call result is correctly consumed by a string concat operator without corrupting the concat state.
/// "len=" . strlen("hello") must produce "len=5".
#[test]
fn test_ffi_extern_call_in_concat_restores_concat_cursor() {
    // Verifies FFI extern call result is correctly consumed by a string concat operator without corrupting the concat state.
    // "len=" . strlen("hello") must produce "len=5".
    let out = compile_and_run(
        r#"<?php
extern function strlen(string $s): int;
echo "len=" . strlen("hello");
"#,
    );
    assert_eq!(out, "len=5");
}

/// Verifies an FFI extern `poll` call within a method body reads arguments from local variables, not from `this`.
#[test]
fn test_ffi_extern_poll_from_method_uses_local_arguments() {
    // Verifies an FFI extern `poll` call within a method body reads arguments from local variables, not from `this`.
    let out = compile_and_run(
        r#"<?php
extern function poll(ptr $fds, int $nfds, int $timeout): int;
class Server {
    public function loop(): void {
        $pollfds = ptr_null();
        $nfds = 0;
        $timeout = 0;
        echo poll($pollfds, $nfds, $timeout);
    }
}
$server = new Server();
$server->loop();
"#,
    );
    assert_eq!(out, "0");
}

/// Regression: FFI extern `poll` called after a loop with internal function calls must not clobber the local `int` `$nfds` argument.
#[test]
fn test_ffi_extern_poll_after_loop_with_calls_preserves_local_int_arg() {
    // Regression: FFI extern `poll` called after a loop with internal function calls must not clobber the local `int` `$nfds` argument.
    let out = compile_and_run(
        r#"<?php
extern "System" {
    function malloc(int $size): ptr;
    function free(ptr $p): void;
    function memset(ptr $dest, int $byte, int $count): ptr;
    function poll(ptr $fds, int $nfds, int $timeout): int;
}
function spin_call(int $x): void {
    if ($x < 0) {
        echo "never";
    }
}
$pollfds = malloc(8);
memset($pollfds, 0, 8);
$active = 0;
$i = 0;
while ($i < 64) {
    spin_call($i);
    $i = $i + 1;
}
$nfds = $active + 1;
echo "n=" . $nfds . ";";
echo "rc=" . poll($pollfds, $nfds, 0);
free($pollfds);
"#,
    );
    assert_eq!(out, "n=1;rc=0");
}

/// Regression: FFI extern `poll` called in a large function with an unrelated associative-array local must not corrupt argument registers.
#[test]
fn test_ffi_extern_poll_in_large_function_survives_unrelated_array_local() {
    // Regression: FFI extern `poll` called in a large function with an unrelated associative-array local must not corrupt argument registers.
    let out = compile_and_run(
        r#"<?php
extern "System" {
    function malloc(int $size): ptr;
    function free(ptr $p): void;
    function memset(ptr $dest, int $byte, int $count): ptr;
    function poll(ptr $fds, int $nfds, int $timeout): int;
}
function run_http_server(): void {
    $pollfds = malloc(8);
    memset($pollfds, 0, 8);
    $active = 0;
    $a0 = 1; $a1 = 2; $a2 = 3; $a3 = 4;
    $a4 = 5; $a5 = 6; $a6 = 7; $a7 = 8;
    $retired = [];
    $retired[0] = $a0 + $a1 + $a2 + $a3 + $a4 + $a5 + $a6 + $a7;
    if ($retired[0] < 0) {
        echo "never";
    }
    $nfds = $active + 1;
    echo "n=" . $nfds . ";";
    echo "rc=" . poll($pollfds, $nfds, 0);
    free($pollfds);
}
run_http_server();
"#,
    );
    assert_eq!(out, "n=1;rc=0");
}

/// Verifies that borrowed C-string temporaries from FFI extern string arguments are freed without leaking.
/// Baseline (strlen declared, not called) and variant (two strlen calls) must have equal alloc/free delta.
#[test]
fn test_ffi_extern_strlen_frees_borrowed_cstr_temp() {
    // Verifies that borrowed C-string temporaries from FFI extern string arguments are freed without leaking.
    // Baseline (strlen declared, not called) and variant (two strlen calls) must have equal alloc/free delta.
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
