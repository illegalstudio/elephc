//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of pointers, including ptr null and is null, ptr null echo, and ptr take address.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures compile to native binaries while malformed or fatal cases assert captured failures.

use crate::support::*;

/// Tests `ptr_null()` returns a null pointer and `ptr_is_null()` correctly identifies it.
#[test]
fn test_ptr_null_and_is_null() {
    let out = compile_and_run(
        r#"<?php
$p = ptr_null();
echo ptr_is_null($p) ? "1" : "0";
"#,
    );
    assert_eq!(out, "1");
}

/// Tests that `ptr_null()` echoes as `0x0` when printed.
#[test]
fn test_ptr_null_echo() {
    let out = compile_and_run(
        r#"<?php
echo ptr_null();
"#,
    );
    assert_eq!(out, "0x0");
}

/// Tests `ptr($var)` takes the address of a local variable and `ptr_is_null()` returns false for a non-null pointer.
#[test]
fn test_ptr_take_address() {
    let out = compile_and_run(
        r#"<?php
$x = 42;
$p = ptr($x);
echo ptr_is_null($p) ? "null" : "not null";
"#,
    );
    assert_eq!(out, "not null");
}

/// Tests `ptr_get()` round-trips the value stored at a pointer's address without modification.
#[test]
fn test_ptr_get_roundtrip() {
    let out = compile_and_run(
        r#"<?php
$x = 42;
$p = ptr($x);
echo ptr_get($p);
"#,
    );
    assert_eq!(out, "42");
}

/// Tests `ptr_set()` writes through a pointer and the original variable reflects the new value.
#[test]
fn test_ptr_set_modifies_variable() {
    let out = compile_and_run(
        r#"<?php
$x = 10;
$p = ptr($x);
ptr_set($p, 99);
echo $x;
"#,
    );
    assert_eq!(out, "99");
}

/// Tests `ptr_offset()` with offset 0 returns the same address as the base pointer.
#[test]
fn test_ptr_offset() {
    let out = compile_and_run(
        r#"<?php
$x = 42;
$p = ptr($x);
$q = ptr_offset($p, 0);
echo ptr_get($q);
"#,
    );
    assert_eq!(out, "42");
}

/// Tests `ptr_cast<int>` changes the pointee type without changing the address; `ptr_get()` still reads the value.
#[test]
fn test_ptr_cast() {
    let out = compile_and_run(
        r#"<?php
$x = 42;
$p = ptr($x);
$q = ptr_cast<int>($p);
echo ptr_get($q);
"#,
    );
    assert_eq!(out, "42");
}

/// Tests that `ptr_cast<int>($p) === $p` holds — a cast does not change pointer identity.
#[test]
fn test_ptr_strict_equal_after_cast() {
    let out = compile_and_run(
        r#"<?php
$x = 42;
$p = ptr($x);
$q = ptr_cast<int>($p);
echo $p === $q ? "1" : "0";
"#,
    );
    assert_eq!(out, "1");
}

/// Tests `ptr_sizeof("int")` returns 8 on 64-bit targets.
#[test]
fn test_ptr_sizeof_int() {
    let out = compile_and_run(
        r#"<?php
echo ptr_sizeof("int");
"#,
    );
    assert_eq!(out, "8");
}

/// Tests `ptr_sizeof("string")` returns 16 (pointer + length on 64-bit).
#[test]
fn test_ptr_sizeof_string() {
    let out = compile_and_run(
        r#"<?php
echo ptr_sizeof("string");
"#,
    );
    assert_eq!(out, "16");
}

/// Tests `ptr_sizeof("float")` returns 8 on 64-bit targets.
#[test]
fn test_ptr_sizeof_float() {
    let out = compile_and_run(
        r#"<?php
echo ptr_sizeof("float");
"#,
    );
    assert_eq!(out, "8");
}

/// Tests `ptr_sizeof("ptr")` returns 8 on 64-bit targets.
#[test]
fn test_ptr_sizeof_ptr() {
    let out = compile_and_run(
        r#"<?php
echo ptr_sizeof("ptr");
"#,
    );
    assert_eq!(out, "8");
}

/// Tests `ptr_sizeof("Point")` for a PHP class with two untyped properties: class_id(8) + 2 × ptr-size(16) = 40.
#[test]
fn test_ptr_sizeof_class() {
    let out = compile_and_run(
        r#"<?php
class Point {
    public $x;
    public $y;
}
echo ptr_sizeof("Point");
"#,
    );
    // class_id(8) + 2 properties * 16 = 40
    assert_eq!(out, "40");
}

/// Tests `ptr_sizeof("Point")` for an extern class with two int properties: 2 × 8 = 16.
#[test]
fn test_ptr_sizeof_extern_class() {
    let out = compile_and_run(
        r#"<?php
extern class Point {
    public int $x;
    public int $y;
}
echo ptr_sizeof("Point");
"#,
    );
    assert_eq!(out, "16");
}

/// Tests that two `ptr_null()` results are strictly equal (`===`) — null pointers share identity.
#[test]
fn test_ptr_strict_equal() {
    let out = compile_and_run(
        r#"<?php
$a = ptr_null();
$b = ptr_null();
echo $a === $b ? "1" : "0";
"#,
    );
    assert_eq!(out, "1");
}

/// Tests that a null pointer and a non-null pointer are strictly not equal (`!==`).
#[test]
fn test_ptr_strict_not_equal() {
    let out = compile_and_run(
        r#"<?php
$x = 1;
$a = ptr_null();
$b = ptr($x);
echo $a !== $b ? "1" : "0";
"#,
    );
    assert_eq!(out, "1");
}

/// Tests that printing a null pointer via `echo $p` produces `0x0` (hex format).
#[test]
fn test_ptr_echo_hex() {
    let out = compile_and_run(
        r#"<?php
$p = ptr_null();
echo $p;
"#,
    );
    assert_eq!(out, "0x0");
}

/// Tests `gettype($ptr)` returns `"pointer"` for a null pointer.
#[test]
fn test_ptr_gettype() {
    let out = compile_and_run(
        r#"<?php
$p = ptr_null();
echo gettype($p);
"#,
    );
    assert_eq!(out, "pointer");
}

/// Tests `empty()` returns true for a null pointer and false for a non-null pointer.
#[test]
fn test_ptr_empty_null_and_non_null() {
    let out = compile_and_run(
        r#"<?php
$x = 1;
$p = ptr($x);
$n = ptr_null();
echo empty($n) ? "1" : "0";
echo empty($p) ? "1" : "0";
"#,
    );
    assert_eq!(out, "10");
}

/// Tests that a pointer passed to a user function survives call/return with its address intact; the function modifies the caller's variable via the pointer.
#[test]
fn test_ptr_in_function() {
    let out = compile_and_run(
        r#"<?php
function double_via_ptr($p) {
    $val = ptr_get($p);
    ptr_set($p, $val * 2);
}
$x = 21;
double_via_ptr(ptr($x));
echo $x;
"#,
    );
    assert_eq!(out, "42");
}

/// Tests that a pointer survives loop iteration: accumulate 1..10 via `ptr_get`/`ptr_set`, result is 55.
#[test]
fn test_ptr_in_loop() {
    let out = compile_and_run(
        r#"<?php
$sum = 0;
$p = ptr($sum);
for ($i = 1; $i <= 10; $i++) {
    ptr_set($p, ptr_get($p) + $i);
}
echo $sum;
"#,
    );
    assert_eq!(out, "55");
}

/// Tests `ptr_write8` and `ptr_read8` round-trip a single byte (255) through a malloc buffer.
#[test]
fn test_ptr_read8_and_write8() {
    let out = compile_and_run(
        r#"<?php
extern function malloc(int $size): ptr;
extern function free(ptr $p): void;

$buf = malloc(1);
ptr_write8($buf, 255);
echo ptr_read8($buf);
free($buf);
"#,
    );
    assert_eq!(out, "255");
}

/// Tests `ptr_write32` and `ptr_read32` round-trip a 32-bit integer (305419896 = 0x12345678) through a malloc buffer.
#[test]
fn test_ptr_read32_and_write32() {
    let out = compile_and_run(
        r#"<?php
extern function malloc(int $size): ptr;
extern function free(ptr $p): void;

$buf = malloc(4);
ptr_write32($buf, 305419896);
echo ptr_read32($buf);
free($buf);
"#,
    );
    assert_eq!(out, "305419896");
}

/// Tests a pointer passed as an argument to a user-defined method preserves its value through the method call; the method writes 999 via `ptr_write32` and the caller reads it back.
#[test]
fn test_ptr_argument_to_user_method_preserves_pointer_value() {
    let out = compile_and_run(
        r#"<?php
extern "System" {
    function malloc(int $size): ptr;
    function free(ptr $p): void;
}
class Writer {
    public function run(): void {
        $mem = malloc(64);
        $this->write($mem, 999);
        echo ptr_read32($mem);
        free($mem);
    }
    public function write(ptr $p, int $value): void {
        ptr_write32($p, $value);
    }
}
$writer = new Writer();
$writer->run();
"#,
    );
    assert_eq!(out, "999");
}

/// Tests `ptr_offset` with a computed local as the byte offset before a `ptr_write32`; verifies no aliasing issue between the computed-address source and the write destination.
#[test]
fn test_ptr_offset_computed_local_before_write32() {
    let out = compile_and_run(
        r#"<?php
extern "System" {
    function malloc(int $size): ptr;
    function free(ptr $p): void;
    function memset(ptr $dest, int $byte, int $count): ptr;
}

function fill(ptr $p, int $slot): void {
    $base = $slot * 8;
    $cell = ptr_offset($p, $base);
    ptr_write32($cell, 1);
}

$m = malloc(64);
memset($m, 0, 64);
fill($m, 0);
echo ptr_read32($m);
free($m);
"#,
    );
    assert_eq!(out, "1");
}

/// Tests `ptr_write16` and `ptr_read16` round-trip a 16-bit integer (4660 = 0x1234) through a malloc buffer.
#[test]
fn test_ptr_read16_and_write16() {
    let out = compile_and_run(
        r#"<?php
extern function malloc(int $size): ptr;
extern function free(ptr $p): void;

$buf = malloc(2);
ptr_write16($buf, 4660);
echo ptr_read16($buf);
free($buf);
"#,
    );
    assert_eq!(out, "4660");
}

/// Tests `ptr_read16` reads little-endian from a malloc buffer: write 0x34 at offset 0 and 0x12 at offset 1, read back 0x1234 = 4660.
#[test]
fn test_ptr_read16_little_endian_from_malloc_block() {
    let out = compile_and_run(
        r#"<?php
extern function malloc(int $size): ptr;
extern function free(ptr $p): void;

$buf = malloc(2);
ptr_write8($buf, 0x34);
ptr_write8(ptr_offset($buf, 1), 0x12);
echo ptr_read16($buf);
free($buf);
"#,
    );
    assert_eq!(out, "4660");
}

/// Tests `ptr_write16` truncates to 16 bits (0x1FFFF → 0xFFFF) and `ptr_read16` zero-extends to a PHP int (65535).
#[test]
fn test_ptr_write16_truncates_and_ptr_read16_zero_extends() {
    let out = compile_and_run(
        r#"<?php
extern function malloc(int $size): ptr;
extern function free(ptr $p): void;

$buf = malloc(2);
ptr_write16($buf, 0x1FFFF);
echo ptr_read16($buf);
free($buf);
"#,
    );
    assert_eq!(out, "65535");
}

/// Tests `ptr_write_string` writes a string to a malloc buffer and `ptr_read_string` reads it back; reports written byte count and the string content.
#[test]
fn test_ptr_write_string_and_read_string_roundtrip() {
    let out = compile_and_run(
        r#"<?php
extern function malloc(int $size): ptr;
extern function free(ptr $p): void;

$buf = malloc(16);
$written = ptr_write_string($buf, "GET /");
$s = ptr_read_string($buf, $written);
echo $written . ":" . $s;
free($buf);
"#,
    );
    assert_eq!(out, "5:GET /");
}

/// Tests `ptr_read_string` reads a sequence of bytes written incrementally via `ptr_write8` into a malloc buffer.
#[test]
fn test_ptr_read_string_from_malloc_block() {
    let out = compile_and_run(
        r#"<?php
extern function malloc(int $size): ptr;
extern function free(ptr $p): void;

$buf = malloc(4);
ptr_write8($buf, 72);
ptr_write8(ptr_offset($buf, 1), 84);
ptr_write8(ptr_offset($buf, 2), 84);
ptr_write8(ptr_offset($buf, 3), 80);
echo ptr_read_string($buf, 4);
free($buf);
"#,
    );
    assert_eq!(out, "HTTP");
}

/// Tests `ptr_read_string` result is an owned PHP string; the source malloc buffer is freed by the callee and the program maintains a clean heap (verified via GC stats).
#[test]
fn test_ptr_read_string_local_return_releases_owned_source() {
    let out = compile_and_run_with_gc_stats(
        r#"<?php
extern function malloc(int $size): ptr;
extern function free(ptr $p): void;

function make_data(): string {
    $buf = malloc(64);
    $data = ptr_read_string($buf, 35);
    free($buf);
    return $data;
}

for ($i = 0; $i < 3; $i++) {
    $x = make_data();
}
echo "done";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    let (allocs, frees) = parse_gc_stats(&out.stderr);
    assert_eq!(allocs, frees, "expected clean heap, got: {}", out.stderr);
    assert_eq!(out.stdout, "done");
}

/// Tests `ptr_read_string` with length 0 returns an empty string.
#[test]
fn test_ptr_read_string_zero_length() {
    let out = compile_and_run(
        r#"<?php
extern function malloc(int $size): ptr;
extern function free(ptr $p): void;

$buf = malloc(1);
echo strlen(ptr_read_string($buf, 0));
free($buf);
"#,
    );
    assert_eq!(out, "0");
}

/// Tests `ptr_write_string` / `ptr_read_string` round-trip preserves a null byte within the string; strlen and individual byte ord values confirm full 3-byte content including the embedded null.
#[test]
fn test_ptr_string_copy_preserves_internal_null_byte() {
    let out = compile_and_run(
        r#"<?php
extern function malloc(int $size): ptr;
extern function free(ptr $p): void;

$buf = malloc(3);
ptr_write_string($buf, "A\0B");
$s = ptr_read_string($buf, 3);
echo strlen($s) . ":" . ord($s[1]) . ":" . ord($s[2]);
free($buf);
"#,
    );
    assert_eq!(out, "3:0:66");
}

/// Tests `function_exists` recognizes pointer built-in functions case-insensitively (PTR_READ16, ptr_write16, ptr_read_string, PTR_WRITE_STRING all return true).
#[test]
fn test_function_exists_recognizes_new_pointer_builtins_case_insensitively() {
    let out = compile_and_run(
        r#"<?php
echo function_exists("PTR_READ16") ? "1" : "0";
echo function_exists("ptr_write16") ? "1" : "0";
echo function_exists("ptr_read_string") ? "1" : "0";
echo function_exists("PTR_WRITE_STRING") ? "1" : "0";
"#,
    );
    assert_eq!(out, "1111");
}

/// Tests `ptr_read_string` with a negative length reports a runtime Fatal error with the expected message.
#[test]
fn test_ptr_read_string_negative_length_reports_runtime_error() {
    let err = compile_and_run_expect_failure(
        r#"<?php
extern function malloc(int $size): ptr;
extern function free(ptr $p): void;

$buf = malloc(1);
echo ptr_read_string($buf, -1);
free($buf);
"#,
    );
    assert!(err.contains("Fatal error: ptr_read_string() length must be non-negative"));
}

/// Tests that `ptr_get` on a null pointer (`ptr_null()`) reports a runtime Fatal error with the expected message.
#[test]
fn test_ptr_null_dereference_reports_runtime_error() {
    let err = compile_and_run_expect_failure(
        r#"<?php
$p = ptr_null();
echo ptr_get($p);
"#,
    );
    assert!(err.contains("Fatal error: null pointer dereference"));
}
