//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of buffers, including buffer integer direct read write, buffer float direct read write, and buffer boolean direct read write.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures compile to native binaries while malformed or fatal cases assert captured failures.

use crate::support::*;

/// Verifies buffer\<int\> read/write via a loop that writes sequential values
/// 1..=buffer_len and sums all four elements.
#[test]
fn test_buffer_int_direct_read_write() {
    let out = compile_and_run(
        "<?php buffer<int> $values = buffer_new<int>(4); for ($i = 0; $i < buffer_len($values); $i = $i + 1) { $values[$i] = $i + 1; } echo $values[0] + $values[1] + $values[2] + $values[3];",
    );
    assert_eq!(out, "10");
}

/// Regression test for issue #497: checked integer arithmetic can supply a
/// runtime-narrowed Mixed index to both buffer writes and reads.
#[test]
fn test_buffer_mixed_arithmetic_index_read_write() {
    let out = compile_and_run(
        "<?php buffer<int> $values = buffer_new<int>(2); $values[$argc - 1] = 41; echo $values[$argc - 1] + 1;",
    );
    assert_eq!(out, "42");
}

/// Verifies the complete Mixed-index contract: strings, floats, and null are
/// converted to int at runtime for both buffer writes and reads.
#[test]
fn test_buffer_mixed_index_runtime_int_conversion() {
    let out = compile_and_run(
        r#"<?php
function mixed_index(mixed $value): mixed {
    return $value;
}
buffer<int> $values = buffer_new<int>(3);
$values[mixed_index("abc")] = 4;
$values[mixed_index(2.9)] = 6;
$values[mixed_index(null)] = $values[mixed_index("abc")] + 1;
echo $values[0], "|", $values[2];
"#,
    );
    assert_eq!(out, "5|6");
}

/// Regression test for issues #497 and #500: a loop-carried checked index can
/// read a packed buffer through an object property without leaking its box.
#[test]
fn test_buffer_packed_property_mixed_index_read_is_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
packed class Thing {
    public int $id;
}
class MapData {
    public $things;
}

buffer<Thing> $things = buffer_new<Thing>(2000);
for ($i = 0; $i < 2000; $i = $i + 1) {
    $things[$i]->id = $i;
}

$map = new MapData();
$map->things = $things;
$sum = 0;
for ($i = 0; $i < 2000; $i = $i + 1) {
    $sum = $sum + $map->things[$i]->id;
}
echo $sum;
buffer_free($things);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "1999000");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected buffer property indices to leave a clean heap, got: {}",
        out.stderr
    );
}

/// Verifies buffer\<float\> stores and retrieves two floating-point values,
/// then casts their sum to int.
#[test]
fn test_buffer_float_direct_read_write() {
    let out = compile_and_run(
        "<?php buffer<float> $values = buffer_new<float>(2); $values[0] = 1.25; $values[1] = 2.75; echo (int) ($values[0] + $values[1]);",
    );
    assert_eq!(out, "4");
}

/// Verifies buffer\<bool\> stores true/false, reads both back, and outputs
/// "1" (only the first value echoed, since PHP treats true as 1 and false as "").
#[test]
fn test_buffer_bool_direct_read_write() {
    let out = compile_and_run(
        "<?php buffer<bool> $flags = buffer_new<bool>(2); $flags[0] = true; $flags[1] = false; echo $flags[0]; echo $flags[1];",
    );
    assert_eq!(out, "1");
}

/// Verifies buffer\<ptr\> stores a null pointer, retrieves it, and confirms
/// ptr_is_null returns 1 (true).
#[test]
fn test_buffer_ptr_direct_read_write() {
    let out = compile_and_run(
        "<?php buffer<ptr> $ptrs = buffer_new<ptr>(1); $ptrs[0] = ptr_null(); echo ptr_is_null($ptrs[0]);",
    );
    assert_eq!(out, "1");
}

/// Verifies that buffer\<Vec2\> (packed class Vec2 with two float fields)
/// allows individual field read/write and sums all four field values
/// across two buffer elements.
#[test]
fn test_buffer_packed_field_access() {
    let out = compile_and_run(
        "<?php packed class Vec2 { public float $x; public float $y; } buffer<Vec2> $points = buffer_new<Vec2>(2); $points[0]->x = 1.5; $points[0]->y = 2.5; $points[1]->x = 3.0; $points[1]->y = 4.0; echo (int) ($points[0]->x + $points[0]->y + $points[1]->x + $points[1]->y);",
    );
    assert_eq!(out, "11");
}

/// Verifies buffer_len returns exactly the size passed to buffer_new.
#[test]
fn test_buffer_len_returns_declared_length() {
    let out = compile_and_run(
        "<?php buffer<int> $values = buffer_new<int>(7); echo buffer_len($values);",
    );
    assert_eq!(out, "7");
}

/// Verifies that buffer\<int\> zero-initializes scalar elements on allocation
/// by echoing three uninitialized elements as "000".
#[test]
fn test_buffer_scalar_elements_are_zero_initialized() {
    let out = compile_and_run("<?php buffer<int> $values = buffer_new<int>(3); echo $values[0]; echo $values[1]; echo $values[2];");
    assert_eq!(out, "000");
}

/// Verifies that buffer\<Vec2\> (packed class with two float fields) zero-initializes
/// all fields on allocation by casting both fields to int and echoing "00".
#[test]
fn test_buffer_packed_fields_are_zero_initialized() {
    let out = compile_and_run(
        "<?php packed class Vec2 { public float $x; public float $y; } buffer<Vec2> $points = buffer_new<Vec2>(1); echo (int) $points[0]->x; echo (int) $points[0]->y;",
    );
    assert_eq!(out, "00");
}

/// Verifies that reading past the declared buffer length produces a fatal
/// "buffer index out of bounds" error rather than reading garbage.
#[test]
fn test_buffer_bounds_check_traps() {
    let err = compile_and_run_expect_failure(
        "<?php buffer<int> $values = buffer_new<int>(1); echo $values[1];",
    );
    assert!(err.contains("buffer index out of bounds"), "{}", err);
}

/// Verifies buffer_free releases memory correctly by writing a value, freeing
/// the buffer, and confirming the value is gone and no crash occurs.
#[test]
fn test_buffer_free_releases_memory() {
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

/// Regression test: repeatedly allocating and freeing large buffers in a loop
/// must not exhaust the heap. Confirms "survived" is echoed after 100 iterations.
#[test]
fn test_buffer_free_in_loop_does_not_exhaust_heap() {
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

/// Verifies that reading from a buffer after buffer_free produces a fatal
/// "use of buffer after buffer_free()" error.
#[test]
fn test_buffer_use_after_free_read_is_fatal() {
    let err = compile_and_run_expect_failure(
        r#"<?php
buffer<int> $buf = buffer_new<int>(5);
buffer_free($buf);
echo $buf[0];
"#,
    );
    assert!(err.contains("use of buffer after buffer_free()"), "{}", err);
}

/// Verifies that writing to a buffer after buffer_free produces a fatal
/// "use of buffer after buffer_free()" error.
#[test]
fn test_buffer_use_after_free_write_is_fatal() {
    let err = compile_and_run_expect_failure(
        r#"<?php
buffer<int> $buf = buffer_new<int>(5);
buffer_free($buf);
$buf[0] = 1;
"#,
    );
    assert!(err.contains("use of buffer after buffer_free()"), "{}", err);
}

/// Verifies that calling buffer_len on a freed buffer produces a fatal
/// "use of buffer after buffer_free()" error.
#[test]
fn test_buffer_len_after_free_is_fatal() {
    let err = compile_and_run_expect_failure(
        r#"<?php
buffer<int> $buf = buffer_new<int>(5);
buffer_free($buf);
echo buffer_len($buf);
"#,
    );
    assert!(err.contains("use of buffer after buffer_free()"), "{}", err);
}

/// Rejects an element count whose byte size wraps `usize` before the runtime
/// can publish the attacker-controlled logical length in a tiny allocation.
#[test]
fn test_buffer_new_rejects_wrapping_element_count() {
    let err = compile_and_run_expect_failure(
        r#"<?php
buffer<int> $buf = buffer_new<int>(2305843009213693953);
echo "unreachable";
"#,
    );
    assert!(
        err.contains("buffer length") || err.contains("buffer size"),
        "expected an explicit invalid buffer-size failure, got: {}",
        err
    );
}

/// Verifies exhausting the finite descriptor registry fails closed with a
/// dedicated diagnostic instead of reporting an unrelated size overflow.
#[test]
fn test_buffer_registry_exhaustion_has_dedicated_diagnostic() {
    let err = compile_and_run_expect_failure(
        r#"<?php
for ($i = 0; $i < 4097; $i++) {
    buffer<int> $buffer = buffer_new<int>(1);
    $buffer[0] = $i;
}
echo "unreachable";
"#,
    );
    assert!(
        err.contains("buffer registry exhausted"),
        "expected the descriptor-capacity diagnostic, got: {err}"
    );
}

/// Verifies freeing one local invalidates an aliased local before a read can
/// access a heap block that may already have been reused for another value.
#[test]
fn test_buffer_alias_read_after_free_is_fatal() {
    let err = compile_and_run_expect_failure(
        r#"<?php
buffer<int> $owner = buffer_new<int>(2);
$alias = $owner;
buffer_free($owner);
echo $alias[0];
"#,
    );
    assert!(err.contains("use of buffer after buffer_free()"), "{}", err);
}

/// Verifies freeing one local invalidates an aliased local before a write can
/// corrupt a subsequently reused heap block.
#[test]
fn test_buffer_alias_write_after_free_is_fatal() {
    let err = compile_and_run_expect_failure(
        r#"<?php
buffer<int> $owner = buffer_new<int>(2);
$alias = $owner;
buffer_free($owner);
$alias[0] = 42;
"#,
    );
    assert!(err.contains("use of buffer after buffer_free()"), "{}", err);
}

/// Verifies `buffer_len()` validates the allocation's liveness instead of
/// trusting a non-null aliased pointer after the owner was freed.
#[test]
fn test_buffer_alias_len_after_free_is_fatal() {
    let err = compile_and_run_expect_failure(
        r#"<?php
buffer<int> $owner = buffer_new<int>(2);
$alias = $owner;
buffer_free($owner);
echo buffer_len($alias);
"#,
    );
    assert!(err.contains("use of buffer after buffer_free()"), "{}", err);
}

/// Verifies `buffer_free()` is idempotent for a local that was already cleared,
/// preserving the runtime heap-free contract instead of writing through null.
#[test]
fn test_buffer_double_free_is_safe() {
    let out = compile_and_run(
        r#"<?php
buffer<int> $buf = buffer_new<int>(2);
buffer_free($buf);
buffer_free($buf);
echo "ok";
"#,
    );
    assert_eq!(out, "ok");
}

/// Verifies a stale alias stays invalid even after the allocator reuses the
/// released block for a new live buffer with a fresh generation.
#[test]
fn test_buffer_alias_stays_invalid_after_heap_block_reuse() {
    let err = compile_and_run_expect_failure(
        r#"<?php
buffer<int> $owner = buffer_new<int>(2);
$stale = $owner;
buffer_free($owner);
buffer<int> $replacement = buffer_new<int>(2);
$replacement[0] = 99;
echo $stale[0];
"#,
    );
    assert!(err.contains("use of buffer after buffer_free()"), "{}", err);
}

/// Verifies that a `buffer_new<int>()` length whose `len * 8` payload size wraps the machine word
/// is rejected. Without the guard, the wrapped product could publish a descriptor for an
/// undersized payload, allowing an in-range logical index to access beyond the allocation.
#[test]
fn test_buffer_new_overflowing_length_is_fatal() {
    let err = compile_and_run_expect_failure(
        r#"<?php
buffer<int> $b = buffer_new<int>(0x2000000000000002);
echo $b[0];
"#,
    );
    assert!(
        err.contains("buffer_new() length is negative or exceeds the maximum buffer size"),
        "{}",
        err
    );
}

/// Verifies that an out-of-range index cannot follow an overflowing `buffer_new<int>()`: the
/// allocation itself aborts, so the read never reaches an undersized payload.
#[test]
fn test_buffer_new_overflow_prevents_out_of_range_read() {
    let out = compile_and_run_capture(
        r#"<?php
buffer<int> $b = buffer_new<int>(0x2000000000000002);
echo "read:", $b[0x100000];
"#,
    );
    assert!(!out.success, "overflowing buffer_new unexpectedly succeeded");
    assert!(
        out.stderr
            .contains("buffer_new() length is negative or exceeds the maximum buffer size"),
        "{}",
        out.stderr
    );
    assert_eq!(out.stdout, "");
}

/// Verifies that an out-of-range write cannot follow an overflowing `buffer_new<int>()` either.
#[test]
fn test_buffer_new_overflow_prevents_out_of_range_write() {
    let err = compile_and_run_expect_failure(
        r#"<?php
buffer<int> $b = buffer_new<int>(0x2000000000000002);
$b[0x40000000] = 1;
echo "wrote";
"#,
    );
    assert!(
        err.contains("buffer_new() length is negative or exceeds the maximum buffer size"),
        "{}",
        err
    );
}

/// Verifies that a negative `buffer_new<int>()` length is rejected with the same controlled fatal
/// instead of being multiplied as an unsigned value into a huge allocation request.
#[test]
fn test_buffer_new_negative_length_is_fatal() {
    let err = compile_and_run_expect_failure(
        r#"<?php
buffer<int> $b = buffer_new<int>(-2);
echo buffer_len($b);
"#,
    );
    assert!(
        err.contains("buffer_new() length is negative or exceeds the maximum buffer size"),
        "{}",
        err
    );
}

/// Positive control: an ordinary small `buffer_new<int>()` still allocates, zero-initializes,
/// reads, and writes, so the length guard did not narrow the common path.
#[test]
fn test_buffer_new_normal_length_still_works() {
    let out = compile_and_run(
        r#"<?php
buffer<int> $b = buffer_new<int>(4);
$b[2] = 42;
echo buffer_len($b), ":", $b[0], ":", $b[2];
"#,
    );
    assert_eq!(out, "4:0:42");
}

/// Verifies the linux-x86_64 runtime rejects invalid payload sizes before publishing a generation
/// handle. The x86_64 code cannot be executed from an aarch64 host, so this asserts on the emitted
/// assembly text and the absence of the legacy in-band buffer header.
#[test]
fn test_x86_64_runtime_buffer_new_carries_length_guard() {
    let target = Target::parse("linux-x86_64").expect("linux-x86_64 is a supported target");
    let runtime_asm = elephc::codegen::generate_runtime(8_388_608, target);
    let marker = "__rt_buffer_new:";
    let start = runtime_asm
        .find(marker)
        .expect("missing assembly label __rt_buffer_new");
    let rest = &runtime_asm[start..];
    let buffer_new = &rest[..rest.find("\n\n").unwrap_or(rest.len())];
    for expected in [
        "js __rt_buffer_new_size_fail",
        "mul rdi",
        "test rdx, rdx",
        "jnz __rt_buffer_new_size_fail",
        "mov r10d, 0xffffffff",
        "cmp rax, r10",
        "ja __rt_buffer_new_size_fail",
    ] {
        assert!(
            buffer_new.contains(expected),
            "x86_64 __rt_buffer_new missing {expected}: {buffer_new}"
        );
    }
    assert!(
        runtime_asm.contains("__rt_buffer_new_size_fail:"),
        "x86_64 runtime is missing the buffer-length fatal handler"
    );
    assert!(
        !buffer_new.contains("add rax, 24"),
        "x86_64 buffer allocation retained the legacy in-band header: {buffer_new}"
    );
    assert!(
        !buffer_new.contains("cmp rax, 0xffffffff"),
        "x86_64 buffer size bound uses a sign-extended immediate: {buffer_new}"
    );
}

/// A packed `int` field accepts a boxed Mixed value that holds an int at runtime: the
/// strict narrowing stores the raw payload, and int arithmetic routed through a Mixed
/// return (its type carries the overflow-to-float promotion) can feed packed storage.
#[test]
fn test_packed_int_field_accepts_mixed_holding_int() {
    let out = compile_and_run(
        "<?php
        packed class Cell { public int $id; }
        function bump(int $x) { return $x + 1; }
        buffer<Cell> $cells = buffer_new<Cell>(1);
        $cells[0]->id = bump(41);
        echo $cells[0]->id;
        buffer_free($cells);
        ",
    );
    assert_eq!(out, "42");
}

/// A packed `int` field receiving a Mixed value that really overflowed to float throws a
/// catchable `TypeError` naming the runtime type — never a silent truncation, and never a
/// box pointer written into fixed field storage.
#[test]
fn test_packed_int_field_mixed_float_throws_type_error() {
    let out = compile_and_run(
        "<?php
        packed class Cell { public int $id; }
        function bump(int $x) { return $x + 1; }
        buffer<Cell> $cells = buffer_new<Cell>(1);
        try {
            $cells[0]->id = bump(PHP_INT_MAX);
            echo \"stored\";
        } catch (TypeError $e) {
            echo get_class($e), \":\", $e->getMessage();
        }
        buffer_free($cells);
        ",
    );
    assert_eq!(
        out,
        "TypeError:Packed field Cell::$id must be of type int, float given"
    );
}

/// The packed-field narrowing is strict: a Mixed string is a `TypeError`, not a numeric
/// coercion — packed fields are a fixed-layout systems extension, not a PHP scalar slot.
#[test]
fn test_packed_int_field_mixed_string_throws_type_error() {
    let out = compile_and_run(
        "<?php
        packed class Cell { public int $id; }
        function pick(mixed $v) { return $v; }
        buffer<Cell> $cells = buffer_new<Cell>(1);
        try {
            $cells[0]->id = pick(\"x\");
        } catch (TypeError $e) {
            echo $e->getMessage();
        }
        buffer_free($cells);
        ",
    );
    assert_eq!(out, "Packed field Cell::$id must be of type int, string given");
}
