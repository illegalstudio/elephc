//! Purpose:
//! Regression tests for issue #479: storing a SUBCLASS over a local whose slot type is a
//! supertype widens the slot to boxed `Mixed`, and the release emitted before an EARLIER
//! store was typed against the concrete slot it saw at the time.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - The widening is deliberate and stays. What was wrong is the release that runs before
//!   the first store: the backend uses the FINAL boxed storage for the whole frame, so an
//!   `Object`-typed load against it is an unbox WITH RETAIN — it hands back the inner
//!   pointer holding a fresh reference, the release cancels exactly that reference, and the
//!   box is never freed. Two blocks per iteration, the box and the object it pinned.
//! - The fix defers that release to `release_local_slot`, which the backend types at the
//!   slot's final storage. Fixing it by suppressing the widening instead is what the first
//!   attempt did, and it breaks twelve `DatePeriod` tests, whose synthetic bodies assign a
//!   `DateTime` in one branch and a `DateTimeImmutable` in the other and rely on the box.
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
/// `catch (\Throwable $e)` types the slot `Object("Throwable")`; storing `Object("TypeError")`
/// over it widens the slot to boxed `Mixed`, because `widened_local_storage_type` has no arm
/// for a pair of object types. The backend then uses that boxed storage for the whole frame —
/// but the release emitted before the CATCH BIND was lowered while the slot still looked
/// concrete, so it unboxes with a retain and cancels only its own reference. Roughly three
/// blocks leaked per iteration.
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
/// mechanism — `Object("Shape")` widened by `Object("Sq")` to `Mixed`, with the release
/// before the FIRST store left typed against the concrete slot.
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
