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

/// Regression for task b4024b9a: scalar buffer elements keep their element type in ternary,
/// `??` and array-literal merges instead of the syntactic `int`, so a `buffer<float>` element is
/// not truncated and a `buffer<bool>` element merged with an int keeps both types.
#[test]
fn test_buffer_scalar_elements_keep_their_type_in_merges() {
    let out = compile_and_run(
        r#"<?php
function from_float(?int $n, buffer<float> $b): void { echo $n ?? $b[0], "|"; }
function from_bool(?int $n, buffer<bool> $b): void { echo $n ?? $b[0], "|"; }
$floats = buffer_new<float>(1);
$floats[0] = 1.5;
echo ($argc > 0) ? $floats[0] : $floats[0], "|";
$list = [$floats[0]];
echo $list[0], "|";
echo ($argc > 0) ? $floats[0] : 2, "|";
from_float(null, $floats);
from_float(3, $floats);
echo "\n";
$flags = buffer_new<bool>(1);
$flags[0] = false;
from_bool(0, $flags);
from_bool(2, $flags);
from_bool(null, $flags);
echo "\n";
"#,
    );
    assert_eq!(out, "1.5|1.5|1.5|1.5|3|\n0|2||\n");
}

/// Regression for task b4024b9a: a nullsafe read of a `buffer<float>` property element merges
/// as the boxed chain result, so a ternary with an int arm does not truncate `1.5` to `1`.
#[test]
fn test_nullsafe_buffer_float_element_keeps_its_type_in_merges() {
    let out = compile_and_run(
        r#"<?php
class S { public buffer<float> $floats; }
$store = new S();
$store->floats = buffer_new<float>(1);
$store->floats[0] = 1.5;
echo ($argc > 0) ? $store?->floats[0] : 0, "|";
$list = [$store?->floats[0]];
echo $list[0], "\n";
"#,
    );
    assert_eq!(out, "1.5|1.5\n");
}

/// Regression: a `?:` arm or an `@`-suppressed ternary arm reading a `?->` chain element is
/// lowered as one chain, so the buffer element survives and a null receiver short-circuits
/// before the index expression runs.
#[test]
fn test_nullsafe_buffer_element_in_merge_arms_stays_one_chain() {
    let out = compile_and_run_capture(
        r#"<?php
class S { public buffer<float> $floats; }
function idx(): int { echo "idx|"; return 0; }
$store = new S();
$store->floats = buffer_new<float>(1);
$store->floats[0] = 1.5;
echo $store?->floats[0] ?: 0, "|";
echo ($argc > 0) ? @$store?->floats[0] : 0, "|";
$none = $argc > 5 ? $store : null;
echo $none?->floats[idx()] ?: "none", "\n";
"#,
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "1.5|1.5|none\n");
    assert_eq!(out.stderr, "");
}

/// Regression for task b4024b9a: a buffer element read through a `??` or assignment receiver
/// keeps the buffer's element type in merges and array literals instead of the syntactic `int`.
#[test]
fn test_buffer_element_through_expression_receivers_keeps_its_type_in_merges() {
    let out = compile_and_run(
        r#"<?php
function from_float(?int $n, buffer<float> $b): void { echo $n ?? ($b ?? $b)[0], "|"; }
function from_bool(?int $n, buffer<bool> $b): void { echo $n ?? ($b ?? $b)[0], "|"; }
$floats = buffer_new<float>(1);
$floats[0] = 1.5;
from_float(null, $floats);
$flags = buffer_new<bool>(1);
$flags[0] = false;
from_bool(null, $flags);
$list = [($alias = $floats)[0], ($floats ?? $floats)[0]];
echo $list[0], "|", $list[1], "|", ($argc > 0) ? ($alias = $floats)[0] : 0, "\n";
"#,
    );
    assert_eq!(out, "1.5||1.5|1.5|1.5\n");
}

/// Regression for task b4024b9a: inside a function, a buffer element read through a property
/// assignment receiver (`($s->buf = make())[0]`, whose call is bound in the assignment's prelude)
/// keeps the buffer's element type in a literal and a ternary.
#[test]
fn test_buffer_element_through_property_assignment_keeps_its_type_in_merges() {
    let out = compile_and_run(
        r#"<?php
function make(): buffer<float> {
    $b = buffer_new<float>(1);
    $b[0] = 1.5;
    return $b;
}
class S { public buffer<float> $buf; }
function show(S $s, int $argc): void {
    $list = [($s->buf = make())[0]];
    echo $list[0], "|";
    echo ($argc > 0) ? ($s->buf = make())[0] : 0, "\n";
}
show(new S(), $argc);
"#,
    );
    assert_eq!(out, "1.5|1.5\n");
}

/// Regression for task b4024b9a: an element read straight from `buffer_new<T>(n)` keeps the
/// buffer's element type in a ternary with an int arm and in an array literal.
#[test]
fn test_buffer_new_element_keeps_its_type_in_merges() {
    let out = compile_and_run(
        r#"<?php
echo ($argc > 0) ? buffer_new<bool>(1)[0] : 1, ":", gettype(($argc > 0) ? buffer_new<bool>(1)[0] : 1);
$list = [buffer_new<float>(1)[0]];
echo "|", gettype($list[0]), "\n";
"#,
    );
    assert_eq!(out, ":boolean|double\n");
}

/// Regression for task b4024b9a: a buffer property read through an assignment receiver
/// (`($t = $s)->buf[0]`) keeps the element type in a ternary and an array literal.
#[test]
fn test_buffer_property_through_assignment_receiver_keeps_its_type_in_merges() {
    let out = compile_and_run(
        r#"<?php
class S { public buffer<float> $buf; public buffer<bool> $flags; }
function show(int $argc): void {
    $s = new S();
    $s->buf = buffer_new<float>(1);
    $s->buf[0] = 1.5;
    $s->flags = buffer_new<bool>(1);
    $s->flags[0] = false;
    echo ($argc > 0) ? ($t = $s)->buf[0] : 0, "|";
    $row = [($t = $s)->buf[0]];
    echo $row[0], "|";
    $flag = ($argc > 0) ? ($t = $s)->flags[0] : 0;
    echo $flag, ":", gettype($flag), "\n";
}
show($argc);
"#,
    );
    assert_eq!(out, "1.5|1.5|:boolean\n");
}

/// Verifies declared buffer properties accept both literal and runtime integer index writes.
#[test]
fn test_buffer_property_declared_literal_and_runtime_index_writes() {
    let out = compile_and_run(
        r#"<?php
class BufferStore {
    public buffer<int> $values;

    public function __construct() {
        $this->values = buffer_new<int>(2);
    }
}

$store = new BufferStore();
$store->values[0] = 11;
$store->values[$argc] = 22;
echo $store->values[0], "|", $store->values[1];
"#,
    );
    assert_eq!(out, "11|22");
}

/// Regression test for issue #625: an untyped property refined to a buffer accepts element writes.
#[test]
fn test_buffer_property_untyped_issue_reproduction_writes_element() {
    let out = compile_and_run(
        r#"<?php
class UntypedBufferStore {
    public $values;
}

$store = new UntypedBufferStore();
$store->values = buffer_new<int>(2);
$store->values[0] = 42;
$store->values[1] = 7;
echo $store->values[0], "|", $store->values[1];
"#,
    );
    assert_eq!(out, "42|7");
}

/// Verifies a method loop can write whole buffer elements through `$this` with a runtime index.
#[test]
fn test_buffer_property_method_loop_writes_elements() {
    let out = compile_and_run(
        r#"<?php
class LoopBufferStore {
    public buffer<int> $values;

    public function __construct(int $length) {
        $this->values = buffer_new<int>($length);
    }

    public function fill(int $length): void {
        for ($i = 0; $i < $length; $i++) {
            $this->values[$i] = $i * 3;
        }
    }
}

$store = new LoopBufferStore(4);
$store->fill(4);
echo $store->values[0], "|", $store->values[1], "|", $store->values[2], "|", $store->values[3];
"#,
    );
    assert_eq!(out, "0|3|6|9");
}

/// Verifies property buffer writes coerce direct and runtime integers into float storage and preserve booleans.
#[test]
fn test_buffer_property_float_and_bool_element_writes() {
    let out = compile_and_run(
        r#"<?php
function runtime_scalar(mixed $value): mixed {
    return $value;
}

class ScalarBufferStore {
    public buffer<float> $floats;
    public buffer<bool> $flags;

    public function __construct() {
        $this->floats = buffer_new<float>(2);
        $this->flags = buffer_new<bool>(2);
    }
}

$store = new ScalarBufferStore();
$store->floats[0] = runtime_scalar(7);
$store->floats[1] = 2.5;
$store->flags[0] = true;
$store->flags[1] = false;
echo $store->floats[0], "|", $store->floats[1], "|", $store->flags[0], "|", $store->flags[1];
"#,
    );
    assert_eq!(out, "7|2.5|1|");
}

/// Verifies compound and postfix updates write their result back into a buffer property element.
#[test]
fn test_buffer_property_compound_and_increment_updates() {
    let out = compile_and_run(
        r#"<?php
class UpdatedBufferStore {
    public buffer<int> $values;

    public function __construct() {
        $this->values = buffer_new<int>(2);
    }
}

$store = new UpdatedBufferStore();
$store->values[0] = 10;
$store->values[1] = 20;
$store->values[0] += 5;
$store->values[1]++;
echo $store->values[0], "|", $store->values[1];
"#,
    );
    assert_eq!(out, "15|21");
}

/// Verifies property buffer writes retain local-write bounds and stale-handle fatal errors.
#[test]
fn test_buffer_property_write_bounds_and_use_after_free_are_fatal() {
    let bounds = compile_and_run_expect_failure(
        r#"<?php
class BoundsBufferStore {
    public buffer<int> $values;

    public function __construct() {
        $this->values = buffer_new<int>(1);
    }
}

$store = new BoundsBufferStore();
$store->values[1] = 9;
"#,
    );
    assert!(bounds.contains("buffer index out of bounds"), "{}", bounds);

    let freed = compile_and_run_expect_failure(
        r#"<?php
class FreedBufferStore {
    public buffer<int> $values;

    public function __construct(buffer<int> $values) {
        $this->values = $values;
    }
}

buffer<int> $values = buffer_new<int>(1);
$store = new FreedBufferStore($values);
buffer_free($values);
$store->values[0] = 9;
"#,
    );
    assert!(
        freed.contains("use of buffer after buffer_free()"),
        "{}",
        freed
    );
}

/// Verifies repeated property buffer writes and reads leave no heap-debug allocations behind.
#[test]
fn test_buffer_property_write_loop_is_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class HeapCleanBufferStore {
    public function __construct(public buffer<int> $values) {}
}

buffer<int> $values = buffer_new<int>(2000);
$store = new HeapCleanBufferStore($values);
for ($i = 0; $i < 2000; $i++) {
    $store->values[$i] = $i;
}
$sum = 0;
for ($i = 0; $i < 2000; $i++) {
    $sum += $store->values[$i];
}
echo $sum;
buffer_free($values);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "1999000");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected property buffer writes to leave a clean heap, got: {}",
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

/// Regression for task f9ea1d1e: `buffer_new<T>()` accepts lengths computed with int arithmetic,
/// which the checker types as `mixed` because PHP promotes an overflowing result to float. Covers
/// a sum, a product and a difference of a parameter, and the shapes of Ivory's collision grid:
/// a property product plus one, `max()` with a multiple of `buffer_len()`, and a multiple of
/// `buffer_len()` alone.
#[test]
fn test_buffer_new_accepts_int_arithmetic_lengths() {
    let out = compile_and_run(
        r#"<?php
packed class Pair { public int $first; public int $second; }

final class Grid
{
    private int $columns;
    private int $rows;
    private buffer<int> $cellStart;
    private buffer<int> $entries;
    private buffer<Pair> $pairs;

    public function __construct(int $columns, int $rows, int $capacity)
    {
        $this->columns = $columns;
        $this->rows = $rows;
        $this->cellStart = buffer_new<int>($this->columns * $this->rows + 1);
        $this->entries = buffer_new<int>($capacity);
        $this->pairs = buffer_new<Pair>($capacity);
    }

    public function grow(int $total): void
    {
        $entries = $this->entries;
        $this->entries = buffer_new<int>(max($total, 2 * buffer_len($entries)));
        buffer_free($entries);
        $pairs = $this->pairs;
        $this->pairs = buffer_new<Pair>(2 * buffer_len($pairs));
        buffer_free($pairs);
    }

    public function sizes(): string
    {
        return buffer_len($this->cellStart) . "," . buffer_len($this->entries) . "," . buffer_len($this->pairs);
    }
}

function sized(int $n): string
{
    $sum = buffer_new<int>($n + 1);
    $product = buffer_new<int>($n * 2);
    $difference = buffer_new<float>($n - 1);
    $sum[$n] = 7;
    return buffer_len($sum) . "," . buffer_len($product) . "," . buffer_len($difference) . "," . $sum[$n];
}

echo sized(5), "|";
$grid = new Grid(4, 3, 2);
echo $grid->sizes(), "|";
$grid->grow(3);
echo $grid->sizes(), "|";
$grid->grow(20);
echo $grid->sizes();
"#,
    );
    assert_eq!(out, "6,10,4,7|13,2,2|13,4,4|13,20,8");
}

/// Runs `examples/buffer-grid`, whose cell table and growing point list are sized with int
/// arithmetic, so the example stays compilable and its printed counts stay correct.
#[test]
fn test_buffer_grid_example_sizes_buffers_with_int_arithmetic() {
    let out = compile_and_run(include_str!("../../examples/buffer-grid/main.php"));
    assert_eq!(out, "cells=12 capacity=16\nused cells=6\ncell (0,0)=3 cell (1,1)=2\n");
}

/// Regression for task f9ea1d1e: a `mixed` or `int|float` length is converted to int at runtime,
/// like a buffer index, so an int, a whole float and a numeric string all size the buffer.
#[test]
fn test_buffer_new_converts_mixed_lengths_at_runtime() {
    let out = compile_and_run(
        r#"<?php
function mixed_len(mixed $value): mixed
{
    return $value;
}

function int_or_float(bool $whole): int|float
{
    return $whole ? 6.0 : 2;
}

$a = buffer_new<int>(mixed_len(3));
$b = buffer_new<int>(mixed_len(4.0));
$c = buffer_new<int>(mixed_len("5"));
$d = buffer_new<int>(int_or_float($argc > 0));
$e = buffer_new<int>(int_or_float($argc < 0));
echo buffer_len($a), ",", buffer_len($b), ",", buffer_len($c), ",", buffer_len($d), ",", buffer_len($e);
"#,
    );
    assert_eq!(out, "3,4,5,6,2");
}

/// Regression for task f9ea1d1e: a boxed int length and a numeric-string length keep their exact
/// value above 2^53, where a float conversion would round them. A zero-size packed element has
/// stride 0, so such a count allocates and `buffer_len()` shows any rounding.
#[test]
fn test_buffer_new_mixed_length_stays_exact_above_float_precision() {
    let out = compile_and_run(
        r#"<?php
packed class Marker {}

function mixed_len(mixed $value): mixed
{
    return $value;
}

$n = $argc << 53;
$b = buffer_new<Marker>($n + 1);
$s = buffer_new<Marker>(mixed_len("9007199254740995"));
echo buffer_len($b), ",", buffer_len($s);
"#,
    );
    assert_eq!(out, "9007199254740993,9007199254740995");
}

/// Regression for task f9ea1d1e: PHP's int conversion saturates an out-of-range numeric string at
/// `PHP_INT_MAX`, whose float equals the string's float 2^63. The length check must still reject
/// it, even for a zero-size packed element whose stride 0 would let the saturated count allocate.
#[test]
fn test_buffer_new_out_of_range_numeric_string_length_is_fatal() {
    let err = compile_and_run_expect_failure(
        r#"<?php
packed class Marker {}

function mixed_len(mixed $value): mixed
{
    return $value;
}

$b = buffer_new<Marker>(mixed_len("9223372036854775808"));
echo "allocated ", buffer_len($b);
"#,
    );
    assert!(
        err.contains("buffer_new() length is negative or exceeds the maximum buffer size"),
        "{}",
        err
    );
}

/// Regression for task f9ea1d1e: a converted length must be a whole number, so a fractional
/// `mixed` length aborts through the length validation instead of truncating to 2 elements.
#[test]
fn test_buffer_new_fractional_mixed_length_is_fatal() {
    let err = compile_and_run_expect_failure(
        r#"<?php
function mixed_len(mixed $value): mixed
{
    return $value;
}

$b = buffer_new<int>(mixed_len(2.5));
echo "allocated ", buffer_len($b);
"#,
    );
    assert!(
        err.contains("buffer_new() length is negative or exceeds the maximum buffer size"),
        "{}",
        err
    );
}

/// Regression for task f9ea1d1e: a NaN length aborts. PHP converts NaN to int 0, so the
/// whole-number check must treat the unordered comparison as a mismatch on every target.
#[test]
fn test_buffer_new_nan_mixed_length_is_fatal() {
    let err = compile_and_run_expect_failure(
        r#"<?php
function mixed_len(mixed $value): mixed
{
    return $value;
}

$b = buffer_new<int>(mixed_len(NAN));
echo "allocated ", buffer_len($b);
"#,
    );
    assert!(
        err.contains("buffer_new() length is negative or exceeds the maximum buffer size"),
        "{}",
        err
    );
}

/// Regression for task f9ea1d1e: `2^62 * 4` overflows to the float 2^64, which PHP's int cast
/// reduces modulo 2^64 to 0. The length conversion must reject it as out of range rather than
/// allocate an empty buffer.
#[test]
fn test_buffer_new_overflowing_arithmetic_length_is_fatal() {
    let err = compile_and_run_expect_failure(
        r#"<?php
$n = $argc << 62;
$b = buffer_new<int>($n * 4);
echo "allocated ", buffer_len($b);
"#,
    );
    assert!(
        err.contains("buffer_new() length is negative or exceeds the maximum buffer size"),
        "{}",
        err
    );
}

/// Regression for task f9ea1d1e: with `--ir-opt`, checked arithmetic over operands the EIR passes
/// fold to constants becomes a raw float constant, and an int `Cast` of a raw float raises PHP's
/// "not representable" warning. The length conversion must stay silent on that path too, so the
/// overflowed `2^62 * 4` reaches the length fatal instead of a throwing error handler, with and
/// without the optimizer.
#[test]
fn test_buffer_new_folded_overflow_length_is_fatal_without_cast_warning() {
    let source = r#"<?php
set_error_handler(function ($level, $message) {
    throw new Exception("warning: " . $message);
});
try {
    $b = buffer_new<int>((($argc & 0) | (1 << 62)) * 4);
    echo "allocated ", buffer_len($b);
} catch (Exception $e) {
    echo $e->getMessage();
}
"#;
    for err in [
        compile_and_run_expect_failure(source),
        without_ir_opt(|| compile_and_run_expect_failure(source)),
    ] {
        assert!(
            err.contains("buffer_new() length is negative or exceeds the maximum buffer size"),
            "{}",
            err
        );
        assert!(!err.contains("not representable"), "{}", err);
    }
}

/// Regression for task f9ea1d1e: converting a boxed arithmetic length releases the box, so sizing
/// a buffer with `$n * 2 + 1` on every iteration leaves a clean heap once each buffer is freed.
#[test]
fn test_buffer_new_arithmetic_length_is_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function sized(int $n): int
{
    $b = buffer_new<int>($n * 2 + 1);
    $len = buffer_len($b);
    buffer_free($b);
    return $len;
}

$total = 0;
for ($i = 0; $i < 100; $i++) {
    $total += sized($i);
}
echo $total;
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "10000");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected arithmetic buffer lengths to leave a clean heap, got: {}",
        out.stderr
    );
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

/// Regression test for packed classes named in class member signatures: a `buffer<Vec2>`
/// property, method parameter, return type, and promoted constructor parameter all resolve, and
/// the program reads and writes elements through `$this->data[$i]->field`. Before the fix the
/// checker registered packed classes after the class schema pass and rejected `Vec2` as an
/// unknown type.
#[test]
fn test_buffer_packed_class_member_signatures_resolve_and_run() {
    let out = compile_and_run(
        r#"<?php
packed class Vec2 { public float $x; public float $y; }

final class Points {
    public buffer<Vec2> $data;

    public function __construct(int $n) {
        $this->data = buffer_new<Vec2>($n);
        for ($i = 0; $i < $n; $i++) {
            $this->data[$i]->x = $i * 1.5;
            $this->data[$i]->y = $i * 2.5;
        }
    }

    public function sumX(): float {
        $sum = 0.0;
        for ($i = 0; $i < buffer_len($this->data); $i++) {
            $sum += $this->data[$i]->x;
        }
        return $sum;
    }

    public function secondY(buffer<Vec2> $points): float {
        return $points[1]->y;
    }

    public function all(): buffer<Vec2> {
        return $this->data;
    }
}

final class Promoted {
    public function __construct(public buffer<Vec2> $data) {}

    public function lastX(): float {
        return $this->data[buffer_len($this->data) - 1]->x;
    }
}

$points = new Points(3);
echo $points->sumX(), "|", $points->secondY($points->data), "|";
$all = $points->all();
echo $all[2]->y, "|";
$promoted = new Promoted($all);
echo $promoted->lastX();
"#,
    );
    assert_eq!(out, "4.5|2.5|5|3");
}

/// Regression test for `ptr<T>` class members whose target is a packed class or an ordinary
/// class the schema pass builds later (`Holder` sorts before `Node`). Both used to fail with
/// "Unknown pointer target type", and an untyped `ptr_null()` must fill a `ptr<T>` slot.
#[test]
fn test_typed_pointer_properties_resolve_packed_and_later_classes() {
    let out = compile_and_run(
        r#"<?php
packed class Vec2 { public float $x; public float $y; }

final class Holder {
    public ptr<Vec2> $vec;
    public ptr<Node> $node;

    public function __construct() {
        $this->vec = ptr_null();
        $this->node = ptr_null();
    }
}

final class Node { public int $id = 7; }

$holder = new Holder();
echo ptr_is_null($holder->vec) ? "null" : "set", "|", ptr_is_null($holder->node) ? "null" : "set";
"#,
    );
    assert_eq!(out, "null|null");
}

/// Verifies a packed class field typed `ptr<Node>` still resolves an ordinary class now that
/// packed classes are registered before the class schema pass builds `Node`.
#[test]
fn test_packed_field_pointer_to_ordinary_class_resolves() {
    let out = compile_and_run(
        r#"<?php
class Node {}
packed class Slot { public ptr<Node> $node; }
buffer<Slot> $slots = buffer_new<Slot>(1);
echo ptr_is_null($slots[0]->node) ? "null" : "set";
"#,
    );
    assert_eq!(out, "null");
}

/// Regression test for a `ptr<T>` packed class field whose target is a builtin enum. The packed
/// and extern prescan now runs before `inject_builtin_enums` registers `SortDirection`, so the
/// builtin enum names must already resolve as pointer targets there; otherwise the packed
/// declaration fails with "Unknown pointer target type: SortDirection". The extern class field,
/// stored as a typed C pointer, is checked alongside to pin that it keeps compiling too.
#[test]
fn test_packed_and_extern_fields_point_to_builtin_enum() {
    let out = compile_and_run(
        r#"<?php
packed class Slot { public ptr<SortDirection> $dir; public int $id; }
extern class Holder { public ptr<SortDirection> $dir; }
buffer<Slot> $slots = buffer_new<Slot>(1);
$slots[0]->id = 3;
echo ptr_is_null($slots[0]->dir) ? "null" : "set", "|", $slots[0]->id, "|";
echo ptr_sizeof("Slot"), "|", ptr_sizeof("Holder");
"#,
    );
    assert_eq!(out, "null|3|16|8");
}
