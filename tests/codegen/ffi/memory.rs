//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of FFI memory, including FFI malloc and free, FFI memset fills raw buffer, and FFI memcpy copies raw buffer.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

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
