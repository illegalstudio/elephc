//! Purpose:
//! Regression tests for issue #479: storing a SUBCLASS over a local whose slot type is a
//! supertype must not widen the slot to boxed `Mixed`, or the release emitted before the
//! store reads a raw object pointer as a box and frees nothing.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Every fixture loops, because the leak is one object pair per REBIND: a single
//!   reassignment is reclaimed by frame cleanup and hides the bug entirely.
//! - The issue reported this as catch-specific ("the previous-value release falls back to
//!   Mixed for catch-bound locals"). It is not: an ordinary local first holding an
//!   interface-typed value leaks identically, which is why the second test is here.
//! - The controls (`$e = null`, `unset($e)`, same-class reassignment) were already clean
//!   before the fix and are kept so a future change cannot fix the leak by disabling the
//!   release path outright.

use crate::support::compile_and_run_with_heap_debug;

/// Asserts the program printed `expected` and left a clean heap under heap debug.
fn assert_clean(out: crate::support::ProgramOutput, expected: &str) {
    assert_eq!(out.stdout, expected, "stderr: {}", out.stderr);
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean heap, got: {}",
        out.stderr
    );
}

/// Issue #479 repro: the caught exception variable reassigned to a new object.
///
/// `catch (\Throwable $e)` types the slot `Object("Throwable")`; `new TypeError` stores
/// `Object("TypeError")` over it. The two are one identical frame representation — a
/// refcounted pointer — but the slot used to widen to `Mixed` anyway, so the release before
/// the store loaded a Mixed box out of a slot holding a raw object pointer and freed
/// nothing. Roughly three blocks leaked per iteration.
#[test]
fn test_issue_479_catch_variable_reassigned_to_object_is_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
for ($n = 0; $n < 50; $n++) {
    try { throw new TypeError("x"); } catch (\Throwable $e) {
        $e = new TypeError("y");
    }
}
echo "done\n";
"#,
    );
    assert_clean(out, "done\n");
}

/// Issue #479 is NOT catch-specific: an ordinary local holding an interface-typed value and
/// then reassigned to a concrete implementation leaks the same way, and by the same
/// mechanism — `Object("Shape")` widened by `Object("Sq")` to `Mixed`.
///
/// Worth a test of its own: fixing only the catch-bound path would leave this one leaking.
#[test]
fn test_issue_479_interface_typed_local_reassigned_to_implementation_is_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
interface Shape { public function area(): int; }
class Sq implements Shape { public function area(): int { return 4; } }

function make(): Shape { return new Sq(); }

$total = 0;
for ($n = 0; $n < 50; $n++) {
    $s = make();
    $s = new Sq();
    $total = $total + $s->area();
}
echo $total, "\n";
"#,
    );
    assert_clean(out, "200\n");
}

/// The controls from the issue, which were already clean before the fix: assigning `null`,
/// `unset()`, and reassigning the SAME class. They pin that the fix widened the storage rule
/// rather than disabling the release-before-store path outright.
///
/// Each loop uses its OWN variable name. Sharing one name would make this a fourth repro
/// instead of a control: the slot is per-name, so the third loop's `TypeError` store would
/// widen the slot the first loop typed `Throwable` and the whole program would leak — which
/// is exactly what it did while all three used `$e`.
#[test]
fn test_issue_479_controls_stay_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
for ($n = 0; $n < 20; $n++) {
    try { throw new TypeError("x"); } catch (\Throwable $nulled) { $nulled = null; }
}
for ($n = 0; $n < 20; $n++) {
    try { throw new TypeError("x"); } catch (\Throwable $dropped) { unset($dropped); }
}
for ($n = 0; $n < 20; $n++) {
    try { throw new TypeError("x"); } catch (TypeError $same) { $same = new TypeError("y"); }
}
echo "done\n";
"#,
    );
    assert_clean(out, "done\n");
}
