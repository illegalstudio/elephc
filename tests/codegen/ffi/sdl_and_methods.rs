//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of FFI, including FFI SDL init and ticks, variadic instance method, and variadic static method.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Includes ignored SDL/platform fixtures that require external SDL2 libraries and target linker support.

use super::*;

/// Verifies SDL2 `SDL_Init` and `SDL_GetTicks` are called via FFI extern block.
/// Requires SDL2 library installed locally (marked #[ignore]).
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

/// Verifies variadic instance method `headAndCount($a, ...$rest)` binds `$a` to 7 and `$rest` to [8, 9].
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

/// Verifies variadic static method `headAndCount($a, ...$rest)` binds `$a` to 7 and `$rest` to [8, 9].
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

/// Verifies SDL2 window creation and destruction via FFI extern block.
/// Requires SDL2 library installed locally and dummy video driver (marked #[ignore]).
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

/// Verifies SDL2 `SDL_GetKeyboardState` returns a non-null pointer via FFI extern block.
/// Requires SDL2 library installed locally (marked #[ignore]).
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

