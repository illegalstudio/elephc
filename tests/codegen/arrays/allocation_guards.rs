//! Purpose:
//! Regression tests for the allocation-size guards in `__rt_array_new` and `__rt_range`.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Before the guards, `capacity * elem_size` wrapped silently: `array_fill(0, 0x2000000000000004, 7)`
//!   allocated 32 bytes, published a 2^61-element header, and the fill loop then ran off the heap
//!   (observed as SIGBUS). `range()` additionally overflows in `end - start + 1`.
//! - Runtime fatals here are process-terminating writes to stderr, so they are asserted through
//!   `compile_and_run_expect_failure`, not through PHP-catchable exceptions. The uncaught
//!   `ValueError` diagnostics reach the same stream: the sizes that reference PHP rejects with a
//!   `ValueError` are now stopped by the lowering guards before any allocator sees them, and
//!   `arrays::size_bounds` asserts those from inside a `catch`.
//! - One test cross-checks the linux-x86_64 runtime text directly, because the emitted x86_64 guard
//!   cannot be executed from an aarch64 host but must still ship in the same change.

use crate::support::*;

/// Returns the generated assembly body for one runtime helper label.
fn runtime_asm_function<'a>(asm: &'a str, label: &str) -> &'a str {
    let marker = format!("{label}:");
    let start = asm
        .find(&marker)
        .unwrap_or_else(|| panic!("missing assembly label {label}"));
    let rest = &asm[start..];
    let end = rest.find("\n\n").unwrap_or(rest.len());
    &rest[..end]
}

/// Verifies `array_fill()` with a count whose `count * 8` payload size wraps the machine word
/// never reaches the allocator at all: the count is past `INT_MAX`, which reference PHP rejects
/// with a catchable `ValueError` before it looks at memory, so the lowering guard raises that
/// instead of letting `__rt_array_new` report the array-size fatal. See `arrays::size_bounds` for
/// the full bounds matrix; the `__rt_array_new` guard behind it is pinned by
/// `test_x86_64_runtime_array_new_carries_size_guard`.
#[test]
fn test_array_fill_overflowing_count_is_fatal() {
    let err = compile_and_run_expect_failure(
        "<?php $a = array_fill(0, 0x2000000000000004, 7); echo count($a);",
    );
    assert!(
        err.contains("Uncaught ValueError: array_fill(): Argument #2 ($count) is too large"),
        "{}",
        err
    );
}

/// Verifies the associative `array_fill()` path is bounded too: a non-zero start routes the caller's
/// count into `__rt_hash_new` as a bucket capacity, where `count * 64` wrapped to 256 bytes and the
/// entry-zeroing loop then ran off the heap (observed as SIGSEGV). The count is past `INT_MAX`, so
/// reference PHP's `ValueError` now keeps it away from that helper entirely.
#[test]
fn test_array_fill_assoc_overflowing_count_is_fatal() {
    let err = compile_and_run_expect_failure(
        "<?php $a = array_fill(5, 0x0400000000000004, 7); echo count($a);",
    );
    assert!(
        err.contains("Uncaught ValueError: array_fill(): Argument #2 ($count) is too large"),
        "{}",
        err
    );
}

/// Verifies `range()` over an interval whose element count overflows a signed 64-bit word aborts.
/// `range(1, PHP_INT_MAX)` keeps a positive count, and reference PHP reports it as an oversized
/// range — a catchable `ValueError` naming the normalized interval — rather than as an allocation
/// failure, so the lowering guard raises that before `__rt_range` sizes anything.
#[test]
fn test_range_overflowing_count_is_fatal() {
    let err = compile_and_run_expect_failure("<?php $a = range(1, PHP_INT_MAX); echo count($a);");
    assert!(
        err.contains(
            "Uncaught ValueError: The supplied range exceeds the maximum array size: start=1 end=9223372036854775807 step=1"
        ),
        "{}",
        err
    );
}

/// Verifies the widest possible interval, whose `end - start + 1` wraps to zero, is rejected with
/// reference PHP's oversized-range `ValueError` rather than silently producing an empty array.
#[test]
fn test_range_wrapping_element_count_is_fatal() {
    let err = compile_and_run_expect_failure(
        "<?php $a = range(PHP_INT_MIN, PHP_INT_MAX); echo count($a);",
    );
    assert!(
        err.contains("The supplied range exceeds the maximum array size"),
        "{}",
        err
    );
}

/// Verifies the descending `range()` path applies the same wrap check as the ascending path.
#[test]
fn test_range_descending_wrapping_element_count_is_fatal() {
    let err = compile_and_run_expect_failure(
        "<?php $a = range(PHP_INT_MAX, PHP_INT_MIN); echo count($a);",
    );
    assert!(
        err.contains("The supplied range exceeds the maximum array size"),
        "{}",
        err
    );
}

/// Verifies `array_pad()` with a target length whose payload size wraps never reaches the
/// allocator at all: reference PHP rejects any `$length` past the maximum allowed array size with a
/// catchable `ValueError`, so the lowering guard raises that before `__rt_array_new` is asked for an
/// impossible capacity. See `arrays::indexed::pad_bounds` for the full bounds matrix.
#[test]
fn test_array_pad_overflowing_length_is_fatal() {
    let err = compile_and_run_expect_failure(
        "<?php $a = array_pad([1, 2], 0x2000000000000004, 0); echo count($a);",
    );
    assert!(
        err.contains(
            "Uncaught ValueError: array_pad(): Argument #2 ($length) must not exceed the maximum allowed array size"
        ),
        "{}",
        err
    );
}

/// Positive control: ordinary array construction, filling, ranges, and padding keep working, so the
/// size guards did not narrow the common path.
#[test]
fn test_normal_array_allocations_still_work() {
    let out = compile_and_run(
        r#"<?php
$a = array_fill(0, 100, 7);
echo count($a), ":", $a[0], ":", $a[99], "|";
foreach (array_fill(5, 3, "x") as $k => $v) { echo "$k=$v,"; }
echo "|";
echo implode(",", range(1, 5)), "|";
echo implode(",", range(5, 1)), "|";
echo implode(",", array_pad([1, 2], 5, 9)), "|";
echo implode(",", array_pad([1, 2], -5, 9));
"#,
    );
    assert_eq!(
        out,
        "100:7:7|5=x,6=x,7=x,|1,2,3,4,5|5,4,3,2,1|1,2,9,9,9|9,9,9,1,2"
    );
}

/// Regression guard for the clean heap-exhaustion path: a capacity that is large but whose payload
/// size is representable must still report heap exhaustion, not the new array-size fatal.
#[test]
fn test_large_but_representable_allocation_still_reports_heap_exhaustion() {
    let err = compile_and_run_expect_failure(
        "<?php $a = array_fill(0, 1000000000, 7); echo count($a);",
    );
    assert!(err.contains("heap memory exhausted"), "{}", err);
}

/// Verifies the linux-x86_64 runtime carries the same `__rt_array_new` size guard as the ARM64
/// runtime. The x86_64 code cannot be executed from an aarch64 host, so this asserts on the emitted
/// assembly text: the clamped capacity, the overflow-checked multiply, the header addition, and the
/// fatal branch must all be present.
#[test]
fn test_x86_64_runtime_array_new_carries_size_guard() {
    let target = Target::parse("linux-x86_64").expect("linux-x86_64 is a supported target");
    let runtime_asm = elephc::codegen::generate_runtime(8_388_608, target);
    let array_new = runtime_asm_function(&runtime_asm, "__rt_array_new");
    for expected in [
        "cmovg rax, rdi",
        "imul rax, rsi",
        "jo __rt_array_cap_overflow",
        "add rax, 24",
    ] {
        assert!(
            array_new.contains(expected),
            "x86_64 __rt_array_new missing {expected}: {array_new}"
        );
    }
    assert!(
        runtime_asm.contains("__rt_array_cap_overflow:"),
        "x86_64 runtime is missing the array-size fatal handler"
    );
    let range = runtime_asm_function(&runtime_asm, "__rt_range");
    assert!(
        range.contains("jle __rt_range_size_fail"),
        "x86_64 __rt_range missing the wrapped element-count guard: {range}"
    );
    let hash_new = runtime_asm_function(&runtime_asm, "__rt_hash_new");
    for expected in [
        "cmovg rax, rdi",
        "imul rax, 64",
        "jo __rt_hash_cap_overflow",
        "mov eax, 56",
        "mov QWORD PTR [rax + 40], r10",
        "mov QWORD PTR [rax + 48], 0",
        "jz __rt_hash_new_done",
    ] {
        assert!(
            hash_new.contains(expected),
            "x86_64 __rt_hash_new missing {expected}: {hash_new}"
        );
    }
}
