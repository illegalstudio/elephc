use crate::support::*;
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

#[test]
fn test_ffi_malloc_and_free() {
    let out = compile_and_run(
        r#"<?php
extern "System" {
    function malloc(int $size): ptr;
    function free(ptr $p): void;
}

$buf = malloc(16);
echo ptr_is_null($buf) ? "null" : "ok";
free($buf);
"#,
    );
    assert_eq!(out, "ok");
}

#[test]
fn test_ffi_memset_fills_raw_buffer() {
    let out = compile_and_run(
        r#"<?php
extern "System" {
    function malloc(int $size): ptr;
    function free(ptr $p): void;
    function memset(ptr $dest, int $byte, int $count): ptr;
}

$buf = malloc(4);
memset($buf, 65, 4);
echo ptr_read8($buf);
echo ",";
echo ptr_read8(ptr_offset($buf, 3));
free($buf);
"#,
    );
    assert_eq!(out, "65,65");
}

#[test]
fn test_ffi_memcpy_copies_raw_buffer() {
    let out = compile_and_run(
        r#"<?php
extern "System" {
    function malloc(int $size): ptr;
    function free(ptr $p): void;
    function memcpy(ptr $dest, ptr $src, int $count): ptr;
}

$src = malloc(4);
$dst = malloc(4);
ptr_write32($src, 305419896);
memcpy($dst, $src, 4);
echo ptr_read32($dst);
free($dst);
free($src);
"#,
    );
    assert_eq!(out, "305419896");
}

#[test]
fn test_ffi_extern_getpid() {
    let out = compile_and_run(
        r#"<?php
extern function getpid(): int;
$pid = getpid();
echo $pid > 0 ? "yes" : "no";
"#,
    );
    assert_eq!(out, "yes");
}

#[test]
fn test_ffi_extern_string_return() {
    let out = compile_and_run(
        r#"<?php
extern function getenv(string $name): string;
$home = getenv("HOME");
echo strlen($home) > 0 ? "ok" : "empty";
"#,
    );
    assert_eq!(out, "ok");
}

#[test]
#[ignore] // requires SDL2 library installed locally
fn test_ffi_sdl_init_and_ticks() {
    let out = compile_and_run(
        r#"<?php
putenv("SDL_VIDEODRIVER=dummy");

extern "SDL2" {
    function SDL_Init(int $flags): int;
    function SDL_Quit(): void;
    function SDL_GetTicks(): int;
    function SDL_Delay(int $ms): void;
}

$SDL_INIT_VIDEO = 32;
echo SDL_Init($SDL_INIT_VIDEO) === 0 ? "init|" : "fail|";
$before = SDL_GetTicks();
SDL_Delay(10);
$after = SDL_GetTicks();
echo $after >= $before ? "ticks" : "bad";
SDL_Quit();
"#,
    );
    assert_eq!(out, "init|ticks");
}

#[test]
fn test_variadic_instance_method() {
    let out = compile_and_run(
        r#"<?php
class Counter {
    public function headAndCount($a, ...$rest) {
        echo $a;
        echo ":";
        echo count($rest);
    }
}

$counter = new Counter();
$counter->headAndCount(7, 8, 9);
"#,
    );
    assert_eq!(out, "7:2");
}

#[test]
fn test_variadic_static_method() {
    let out = compile_and_run(
        r#"<?php
class Counter {
    public static function headAndCount($a, ...$rest) {
        echo $a;
        echo ":";
        echo count($rest);
    }
}

Counter::headAndCount(7, 8, 9);
"#,
    );
    assert_eq!(out, "7:2");
}

#[test]
#[ignore] // requires SDL2 library installed locally
fn test_ffi_sdl_window_with_dummy_driver() {
    let out = compile_and_run(
        r#"<?php
putenv("SDL_VIDEODRIVER=dummy");

extern "SDL2" {
    function SDL_Init(int $flags): int;
    function SDL_Quit(): void;
    function SDL_CreateWindow(string $title, int $x, int $y, int $w, int $h, int $flags): ptr;
    function SDL_DestroyWindow(ptr $window): void;
}

$SDL_INIT_VIDEO = 32;
if (SDL_Init($SDL_INIT_VIDEO) != 0) {
    echo "init fail";
    exit(1);
}

$window = SDL_CreateWindow("test", 0, 0, 64, 64, 0);
echo ptr_is_null($window) ? "null" : "ok";
if (!ptr_is_null($window)) {
    SDL_DestroyWindow($window);
}
SDL_Quit();
"#,
    );
    assert_eq!(out, "ok");
}

#[test]
#[ignore] // requires SDL2 library installed locally
fn test_ffi_sdl_keyboard_state_pointer() {
    let out = compile_and_run(
        r#"<?php
putenv("SDL_VIDEODRIVER=dummy");

extern "SDL2" {
    function SDL_Init(int $flags): int;
    function SDL_Quit(): void;
    function SDL_PumpEvents(): void;
    function SDL_GetKeyboardState(ptr $numkeys): ptr;
}

$SDL_INIT_VIDEO = 32;
if (SDL_Init($SDL_INIT_VIDEO) != 0) {
    echo "init fail";
    exit(1);
}

SDL_PumpEvents();
$keys = SDL_GetKeyboardState(ptr_null());
echo ptr_is_null($keys) ? "null" : "ok";
SDL_Quit();
"#,
    );
    assert_eq!(out, "ok");
}

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
