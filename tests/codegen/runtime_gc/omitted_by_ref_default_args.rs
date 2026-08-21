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
//! - EVERY MATERIALIZATION PATH IS COVERED, and the routing was CHECKED rather than assumed
//!   (an earlier version of this file claimed `$counter->bump($i)` exercised the
//!   receiver-REGISTER lowering; it does not — a typed local receiver goes through the same
//!   direct-call materializer a plain function does). The four stagers and the fixture that
//!   genuinely reaches each one:
//!
//!   | materializer | fixture |
//!   |---|---|
//!   | direct call | `test_omitted_by_ref_default_arg_is_balanced` (and the method/static ones) |
//!   | static method (hidden called-class id) | `test_omitted_by_ref_default_arg_on_static_method_is_balanced` |
//!   | receiver REGISTER (`nested_call_reg`) | `test_omitted_by_ref_default_arg_on_mixed_receiver_is_balanced` — a `mixed`-typed receiver forces the register dispatch (verified in the emitted assembly: `mov x19, x1` for the receiver, with the cell pushed before it) |
//!   | receiver LOCAL (`parent::m()`) | `test_omitted_by_ref_default_arg_on_parent_call_is_balanced` |
//!
//!   A refcounted cell type (`array`) has its own fixture too, because releasing the cell's
//!   CONTENT is a separate step from releasing the cell.
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

/// An INSTANCE METHOD on a typed local omitting the same argument. This routes through the
/// DIRECT-CALL materializer (a statically resolved receiver needs no register dispatch), so
/// it covers the ordinary method shape a user writes — see
/// `test_omitted_by_ref_default_arg_on_mixed_receiver_is_balanced` for the receiver-register
/// stager.
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

/// THE RECEIVER-REGISTER STAGER. A `mixed`-typed receiver cannot be dispatched statically, so
/// the lowering unboxes it into the reserved callee-saved nested-call register and stages the
/// arguments around it — including the caller-side cell for the omitted `int &$out = 1`.
///
/// It asserts the RETURN VALUE, not just the heap: `$this->base + $x` is only right if the
/// receiver register survived the cell block. A cell block that clobbered it would enter the
/// method with the cell's address as `$this` and read `base` out of the stack.
#[test]
fn test_omitted_by_ref_default_arg_on_mixed_receiver_is_balanced() {
    assert_balanced(
        r#"<?php
final class Svc {
    public int $base = 5;
    public function run(int $x, int &$out = 1): int { $out = $x; return $this->base + $x; }
}
function call_it(mixed $o): int { return $o->run(2); }
function main(): void {
    $svc = new Svc();
    $total = 0;
    for ($i = 0; $i < 20; $i++) { $total += call_it($svc); }
    echo $total;
}
main();
"#,
        "140",
    );
}

/// THE RECEIVER-LOCAL STAGER: `parent::run()` from a child method, with the by-reference
/// default omitted. `$this` comes from a local slot rather than a register, which is a third
/// cell-block placement (the receiver is staged after the block, at its own offset).
#[test]
fn test_omitted_by_ref_default_arg_on_parent_call_is_balanced() {
    assert_balanced(
        r#"<?php
class Base {
    public int $base = 5;
    public function run(int $x, int &$out = 1): int { $out = $x; return $this->base + $x; }
}
final class Child extends Base {
    public function go(int $x): int { return parent::run($x); }
}
function main(): void {
    $child = new Child();
    $total = 0;
    for ($i = 0; $i < 20; $i++) { $total += $child->go($i); }
    echo $total;
}
main();
"#,
        "290",
    );
}

/// A NULLABLE receiver (`$svc?->run($i)`) with the by-reference default omitted: the null
/// check short-circuits, the non-null arm stages a cell, and the method still runs on the
/// right object.
#[test]
fn test_omitted_by_ref_default_arg_on_nullsafe_call_is_balanced() {
    assert_balanced(
        r#"<?php
final class Svc {
    public int $base = 5;
    public function run(int $x, int &$out = 1): int { $out = $x; return $this->base + $x; }
}
function pick(bool $yes): ?Svc { return $yes ? new Svc() : null; }
function main(): void {
    $hit = pick(true);
    $miss = pick(false);
    $total = 0;
    for ($i = 0; $i < 20; $i++) { $total += $hit?->run($i); }
    echo $total, ",", ($miss?->run(1) === null ? "null" : "?");
}
main();
"#,
        "290,null",
    );
}
