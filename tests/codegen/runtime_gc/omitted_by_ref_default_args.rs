//! Purpose:
//! Heap-balance coverage for a call that OMITS an optional by-reference argument
//! (`f($x)` against `f($x, int &$out = 7)`). The callee still needs an address to write
//! through, so the caller materializes a cell for it — and since no caller variable stands
//! behind that cell, nothing ever reads it back.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - THIS IS A REGRESSION SUITE FOR A REAL LEAK. That cell used to be
//!   `__rt_heap_alloc(16)` (`materialize_temporary_ref_arg_cell`) that nothing freed: one
//!   16-byte block per call, unbounded in a loop. It is now a caller-stack cell in the same
//!   block as the scalar-to-Mixed writeback cells, released once after the call
//!   (`src/codegen/lower_inst/reference_arguments.rs`). Measured before the fix, three calls
//!   leaked three blocks in every shape below; after it, each is balanced.
//! - The loop counts are deliberately larger than one so a per-call leak cannot hide inside
//!   the fixed startup allocations `--gc-stats` also reports.
//! - EVERY MATERIALIZATION PATH IS COVERED, not just plain function calls: a direct call, an
//!   instance method (receiver in a register), a static method, and a refcounted cell type
//!   (`array`), because the four lowerings stage by-reference arguments through separate
//!   code paths and only a per-path fixture proves each one releases its cell.
//! - Every expected stdout value is real `php` 8.5 output for the same source.

use crate::support::{compile_and_run_with_gc_stats, compile_and_run_with_heap_debug, parse_gc_stats};

/// Asserts a program prints `expected` and allocates exactly as many heap blocks as it frees.
fn assert_balanced(source: &str, expected: &str) {
    let output = compile_and_run_with_gc_stats(source);
    assert_eq!(output.stdout, expected, "stderr: {}", output.stderr);
    let (allocs, frees) = parse_gc_stats(&output.stderr);
    assert_eq!(
        allocs, frees,
        "omitting an optional by-reference argument must not leak; stderr: {}",
        output.stderr
    );
}

/// A plain function call that omits an `int &$out = 7` argument, twenty times over: the
/// callee reads the default, writes through the address, and the caller discards the cell.
#[test]
fn test_omitted_by_ref_default_arg_is_balanced() {
    assert_balanced(
        r#"<?php
function step(int $x, int &$out = 7): int { $seen = $out; $out = $x * 2; return $seen; }
function main(): void {
    $total = 0;
    for ($i = 0; $i < 20; $i++) { $total += step($i); }
    echo $total;
}
main();
"#,
        "140",
    );
}

/// The same call under `--heap-debug`, which reports the LIVE blocks at exit rather than the
/// alloc/free totals: a leaked cell shows up as a named live allocation, not just a count.
#[test]
fn test_omitted_by_ref_default_arg_leaves_a_clean_heap() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function step(int $x, int &$out = 7): int { $out = $x; return $out; }
for ($i = 0; $i < 20; $i++) { step($i); }
echo "done";
"#,
    );
    assert_eq!(out.stdout, "done", "stderr: {}", out.stderr);
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap, got: {}",
        out.stderr
    );
}

/// An INSTANCE METHOD omitting the same argument: the receiver arrives in a register, which
/// is its own materialization path with its own cell block.
#[test]
fn test_omitted_by_ref_default_arg_on_instance_method_is_balanced() {
    assert_balanced(
        r#"<?php
final class Counter {
    public int $base = 10;
    public function bump(int $by, int &$out = 3): int { $seen = $out; $out = $this->base + $by; return $seen; }
}
function main(): void {
    $counter = new Counter();
    $total = 0;
    for ($i = 0; $i < 20; $i++) { $total += $counter->bump($i); }
    echo $total;
}
main();
"#,
        "60",
    );
}

/// A STATIC METHOD omitting the same argument: a third materialization path (the hidden
/// called-class id argument shifts every offset the cell block is addressed against).
#[test]
fn test_omitted_by_ref_default_arg_on_static_method_is_balanced() {
    assert_balanced(
        r#"<?php
final class Maker {
    public static function make(int $value, int &$out = 5): int { $seen = $out; $out = $value; return $seen; }
}
function main(): void {
    $total = 0;
    for ($i = 0; $i < 20; $i++) { $total += Maker::make($i); }
    echo $total;
}
main();
"#,
        "100",
    );
}

/// A REFCOUNTED cell type. The discarded cell holds an `array` the callee appended to, so
/// releasing the cell means releasing that array — the same ownership rule the writeback
/// cells follow. A missing release leaks the array; an over-release would crash instead.
#[test]
fn test_omitted_by_ref_default_array_arg_is_balanced() {
    assert_balanced(
        r#"<?php
function collect(int $x, array &$out = []): int { $out[] = $x; return count($out); }
function main(): void {
    $sizes = 0;
    for ($i = 0; $i < 20; $i++) { $sizes += collect($i); }
    $kept = [9];
    for ($i = 0; $i < 3; $i++) { collect($i, $kept); }
    echo $sizes, ":", count($kept);
}
main();
"#,
        "20:4",
    );
}

/// BEHAVIOUR, not just balance: the callee really does see the declared default through the
/// discarded cell, and passing the argument still writes back into the caller's variable.
/// A cell that was silently zeroed (or shared between calls) would pass a leak test and fail
/// this one.
#[test]
fn test_omitted_by_ref_default_arg_keeps_php_semantics() {
    assert_balanced(
        r#"<?php
function step(int $x, int &$out = 7): int { $seen = $out; $out = $x * 2; return $seen; }
function main(): void {
    echo step(1), ",", step(2), ",";
    $caller = 100;
    echo step(3, $caller), ",", $caller;
}
main();
"#,
        "7,7,100,6",
    );
}
