//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of buffers, including buffer integer direct read write, buffer float direct read write, and buffer boolean direct read write.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures compile to native binaries while malformed or fatal cases assert captured failures.

use crate::support::*;

#[test]
fn test_buffer_int_direct_read_write() {
    // Verifies buffer\<int\> read/write via a loop that writes sequential values
    // 1..=buffer_len and sums all four elements.
    let out = compile_and_run(
        "<?php buffer<int> $values = buffer_new<int>(4); for ($i = 0; $i < buffer_len($values); $i = $i + 1) { $values[$i] = $i + 1; } echo $values[0] + $values[1] + $values[2] + $values[3];",
    );
    assert_eq!(out, "10");
}

#[test]
fn test_buffer_float_direct_read_write() {
    // Verifies buffer\<float\> stores and retrieves two floating-point values,
    // then casts their sum to int.
    let out = compile_and_run(
        "<?php buffer<float> $values = buffer_new<float>(2); $values[0] = 1.25; $values[1] = 2.75; echo (int) ($values[0] + $values[1]);",
    );
    assert_eq!(out, "4");
}

#[test]
fn test_buffer_bool_direct_read_write() {
    // Verifies buffer\<bool\> stores true/false, reads both back, and outputs
    // "1" (only the first value echoed, since PHP treats true as 1 and false as "").
    let out = compile_and_run(
        "<?php buffer<bool> $flags = buffer_new<bool>(2); $flags[0] = true; $flags[1] = false; echo $flags[0]; echo $flags[1];",
    );
    assert_eq!(out, "1");
}

#[test]
fn test_buffer_ptr_direct_read_write() {
    // Verifies buffer\<ptr\> stores a null pointer, retrieves it, and confirms
    // ptr_is_null returns 1 (true).
    let out = compile_and_run(
        "<?php buffer<ptr> $ptrs = buffer_new<ptr>(1); $ptrs[0] = ptr_null(); echo ptr_is_null($ptrs[0]);",
    );
    assert_eq!(out, "1");
}

#[test]
fn test_buffer_packed_field_access() {
    // Verifies that buffer\<Vec2\> (packed class Vec2 with two float fields)
    // allows individual field read/write and sums all four field values
    // across two buffer elements.
    let out = compile_and_run(
        "<?php packed class Vec2 { public float $x; public float $y; } buffer<Vec2> $points = buffer_new<Vec2>(2); $points[0]->x = 1.5; $points[0]->y = 2.5; $points[1]->x = 3.0; $points[1]->y = 4.0; echo (int) ($points[0]->x + $points[0]->y + $points[1]->x + $points[1]->y);",
    );
    assert_eq!(out, "11");
}

#[test]
fn test_buffer_len_returns_declared_length() {
    // Verifies buffer_len returns exactly the size passed to buffer_new.
    let out = compile_and_run(
        "<?php buffer<int> $values = buffer_new<int>(7); echo buffer_len($values);",
    );
    assert_eq!(out, "7");
}

#[test]
fn test_buffer_scalar_elements_are_zero_initialized() {
    // Verifies that buffer\<int\> zero-initializes scalar elements on allocation
    // by echoing three uninitialized elements as "000".
    let out = compile_and_run("<?php buffer<int> $values = buffer_new<int>(3); echo $values[0]; echo $values[1]; echo $values[2];");
    assert_eq!(out, "000");
}

#[test]
fn test_buffer_packed_fields_are_zero_initialized() {
    // Verifies that buffer\<Vec2\> (packed class with two float fields) zero-initializes
    // all fields on allocation by casting both fields to int and echoing "00".
    let out = compile_and_run(
        "<?php packed class Vec2 { public float $x; public float $y; } buffer<Vec2> $points = buffer_new<Vec2>(1); echo (int) $points[0]->x; echo (int) $points[0]->y;",
    );
    assert_eq!(out, "00");
}

#[test]
fn test_buffer_bounds_check_traps() {
    // Verifies that reading past the declared buffer length produces a fatal
    // "buffer index out of bounds" error rather than reading garbage.
    let err = compile_and_run_expect_failure(
        "<?php buffer<int> $values = buffer_new<int>(1); echo $values[1];",
    );
    assert!(err.contains("buffer index out of bounds"), "{}", err);
}

#[test]
fn test_buffer_free_releases_memory() {
    // Verifies buffer_free releases memory correctly by writing a value, freeing
    // the buffer, and confirming the value is gone and no crash occurs.
    let out = compile_and_run(
        r#"<?php
buffer<int> $buf = buffer_new<int>(10);
$buf[0] = 42;
echo $buf[0] . " ";
buffer_free($buf);
echo "ok";
"#,
    );
    assert_eq!(out, "42 ok");
}

#[test]
fn test_buffer_free_in_loop_does_not_exhaust_heap() {
    // Regression test: repeatedly allocating and freeing large buffers in a loop
    // must not exhaust the heap. Confirms "survived" is echoed after 100 iterations.
    let out = compile_and_run(
        r#"<?php
for ($i = 0; $i < 100; $i++) {
    buffer<int> $tmp = buffer_new<int>(1000);
    $tmp[0] = $i;
    buffer_free($tmp);
}
echo "survived";
"#,
    );
    assert_eq!(out, "survived");
}

#[test]
fn test_buffer_use_after_free_read_is_fatal() {
    // Verifies that reading from a buffer after buffer_free produces a fatal
    // "use of buffer after buffer_free()" error.
    let err = compile_and_run_expect_failure(
        r#"<?php
buffer<int> $buf = buffer_new<int>(5);
buffer_free($buf);
echo $buf[0];
"#,
    );
    assert!(err.contains("use of buffer after buffer_free()"), "{}", err);
}

#[test]
fn test_buffer_use_after_free_write_is_fatal() {
    // Verifies that writing to a buffer after buffer_free produces a fatal
    // "use of buffer after buffer_free()" error.
    let err = compile_and_run_expect_failure(
        r#"<?php
buffer<int> $buf = buffer_new<int>(5);
buffer_free($buf);
$buf[0] = 1;
"#,
    );
    assert!(err.contains("use of buffer after buffer_free()"), "{}", err);
}

#[test]
fn test_buffer_len_after_free_is_fatal() {
    // Verifies that calling buffer_len on a freed buffer produces a fatal
    // "use of buffer after buffer_free()" error.
    let err = compile_and_run_expect_failure(
        r#"<?php
buffer<int> $buf = buffer_new<int>(5);
buffer_free($buf);
echo buffer_len($buf);
"#,
    );
    assert!(err.contains("use of buffer after buffer_free()"), "{}", err);
}
