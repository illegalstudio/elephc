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
fn test_ffi_extern_call_user_func_string_callback() {
    let out = compile_and_run(
        r#"<?php
extern function strlen(string $s): int;
echo call_user_func("STRLEN", "hello");
"#,
    );
    assert_eq!(out, "5");
}

#[test]
fn test_ffi_extern_call_user_func_array_string_callback() {
    let out = compile_and_run(
        r#"<?php
extern function strlen(string $s): int;
echo call_user_func_array("STRLEN", ["hello"]);
"#,
    );
    assert_eq!(out, "5");
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
fn test_ffi_extern_poll_from_method_uses_local_arguments() {
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

#[test]
fn test_ffi_extern_poll_after_loop_with_calls_preserves_local_int_arg() {
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

#[test]
fn test_ffi_extern_poll_in_large_function_survives_unrelated_array_local() {
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
