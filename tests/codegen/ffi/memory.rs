//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of FFI memory, including FFI malloc and free, FFI memset fills raw buffer, and FFI memcpy copies raw buffer.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

/// Verifies that `malloc` returns a valid non-null pointer for a 16-byte allocation
/// and that `free()` can be called on it without error.
///
/// Fixture: extern "System" functions malloc(16) and free(ptr), ptr_is_null check.
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

/// Verifies that `memset` fills a raw buffer with a byte value repeated count times.
///
/// Fixture: malloc(4) buffer, memset($buf, 65, 4) writes byte 65 ('A') to all 4 bytes.
/// Assertions: first and last byte both equal 65 (ASCII 'A').
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

/// Verifies that `memset` accepts an arithmetic expression as the count argument.
///
/// Fixture: malloc(4) buffer, active=1, memset($buf, 65, $active + 1) writes byte 65
/// to 2 bytes. Assertions: first byte=65, second=65, third=0 (unwritten).
#[test]
fn test_ffi_memset_accepts_arithmetic_count_argument() {
    let out = compile_and_run(
        r#"<?php
extern "System" {
    function malloc(int $size): ptr;
    function free(ptr $p): void;
    function memset(ptr $dest, int $byte, int $count): ptr;
}

$buf = malloc(4);
$active = 1;
memset($buf, 65, $active + 1);
echo ptr_read8($buf) . "," . ptr_read8(ptr_offset($buf, 1)) . "," . ptr_read8(ptr_offset($buf, 2));
free($buf);
"#,
    );
    assert_eq!(out, "65,65,0");
}

/// Verifies that `memcpy` copies raw bytes from source to destination buffer.
///
/// Fixture: malloc(4) src and dst, ptr_write32 writes u32 value 305419896 to src,
/// memcpy copies 4 bytes to dst, ptr_read32 reads back the same value.
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

/// Verifies that a plain `extern function` returning `int` is resolved and called correctly.
///
/// Fixture: extern function getpid() returning int, asserts the returned pid is positive.
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

/// Verifies that an extern function with a `string` parameter and `string` return type
/// is resolved and called correctly.
///
/// Fixture: extern function getenv("HOME") returning string, asserts strlen > 0.
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
