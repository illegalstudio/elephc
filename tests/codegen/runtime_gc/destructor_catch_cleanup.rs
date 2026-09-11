//! Purpose:
//! Verifies that a throw raised by an implicitly run `__destruct` reaches a `try` in the SAME
//! frame, for the two retirement sites the direct-call and IIFE regressions do not cover: a
//! fresh container's owned child, and a write that retires the value a name previously held.
//!
//! Called from:
//! - The runtime GC codegen integration suite.
//!
//! Key details:
//! - Every fixture runs under heap debug, so a stranded owner shows up as a leak summary.
//! - Destructor output pins WHEN each payload is destroyed, which a leak summary alone cannot.
//! - The `try` sits INSIDE the function whose retirement is under test, so the unwind that
//!   reaches the catch is that retirement alone rather than a whole activation teardown.
//! - Each fixture runs twice so a per-call imbalance shows up in the leak summary.
//! - Where two independent owners are retired at the SAME statement, the assertions count
//!   markers instead of pinning a total order: which of the two runs first is an ownership
//!   implementation detail, while "both ran, exactly once each, and the catch was entered" is
//!   the behavior under test. Where PHP fixes the order (a retirement followed by the handler
//!   that catches it), the exact string is asserted.

use crate::support::*;

/// Counts non-overlapping occurrences of `marker` in `output`.
///
/// Used by the fixtures whose retirement order is deliberately not pinned, so that a per-call
/// imbalance (one destructor running twice, or not at all) still fails.
fn marker_count(output: &str, marker: &str) -> usize {
    output.matches(marker).count()
}

/// A throwing destructor on a fresh array's owned child is caught in the frame that built it.
///
/// The array temporary is rooted across the call and retired after it, and retiring the
/// container retires the child it owns. The discarded `ResultPayload` is retired at the same
/// statement, so its marker proves the unwind retires it exactly once instead of stranding it,
/// without this test claiming which of the two owners goes first.
#[test]
fn test_core_fresh_array_child_destructor_reaches_a_same_frame_catch() {
    let source = r#"<?php
class ResultPayload {
    public function __destruct() { echo 'result|'; }
}
class ThrowingChild {
    public function __destruct() { echo 'child|'; throw new RuntimeException('child'); }
}
function buildFromChildren(array $markers): ResultPayload { return new ResultPayload(); }
function buildWithArrayChildCatch(): string {
    try {
        buildFromChildren([new ThrowingChild()]);
        echo 'unreached|';
        return 'no';
    } catch (RuntimeException $error) {
        echo 'caught|';
        return 'arr';
    }
}
echo buildWithArrayChildCatch(), ':', buildWithArrayChildCatch();
"#;
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    // The array temporary and the discarded result are two independent owners retired at the
    // same statement, so only their COUNTS and the handler entry are pinned here.
    let tagged = compile_and_run_tagged(source);
    for output in [out.stdout.as_str(), tagged.as_str()] {
        assert_eq!(marker_count(output, "child|"), 2, "{output:?}");
        assert_eq!(marker_count(output, "result|"), 2, "{output:?}");
        assert_eq!(marker_count(output, "caught|"), 2, "{output:?}");
        assert_eq!(marker_count(output, "unreached|"), 0, "{output:?}");
        assert!(output.ends_with("arr"), "{output:?}");
        assert_eq!(marker_count(output, "arr"), 2, "{output:?}");
    }
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A throwing destructor run by `unset` is caught in the frame that retired the value.
///
/// `unset` retires whatever the name held, so the destructor runs at that statement rather than
/// at frame teardown. The loop repeats the retirement so a per-call imbalance would surface.
#[test]
fn test_core_unset_retirement_destructor_reaches_a_same_frame_catch() {
    let source = r#"<?php
class ThrowingHeld {
    public function __destruct() { echo 'held|'; throw new RuntimeException('held'); }
}
function replaceWithSameFrameCatch(): string {
    $held = new ThrowingHeld();
    try {
        unset($held);
        echo 'unreached|';
        return 'no';
    } catch (RuntimeException $error) {
        echo 'caught|';
        return 'set';
    }
}
echo replaceWithSameFrameCatch(), ':', replaceWithSameFrameCatch();
"#;
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(
        out.stdout, "held|caught|set:held|caught|set",
        "{}", out.stderr,
    );
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
    assert_eq!(
        compile_and_run_tagged(source),
        "held|caught|set:held|caught|set",
    );
}

/// A throwing destructor run by REBINDING a local is caught in the frame that rebound it.
///
/// Widening the name to a scalar retires the object it held, which is the same retirement site
/// `unset` uses, reached through an ordinary assignment instead.
#[test]
fn test_core_rebinding_a_local_runs_its_destructor_into_a_same_frame_catch() {
    let source = r#"<?php
class ThrowingRebound {
    public function __destruct() { echo 'rebound|'; throw new RuntimeException('rebound'); }
}
function rebindWithSameFrameCatch(): string {
    $held = new ThrowingRebound();
    try {
        $held = 42;
        echo 'unreached|', $held;
        return 'no';
    } catch (RuntimeException $error) {
        echo 'caught|';
        return 'rebind';
    }
}
echo rebindWithSameFrameCatch(), ':', rebindWithSameFrameCatch();
"#;
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(
        out.stdout, "rebound|caught|rebind:rebound|caught|rebind",
        "{}", out.stderr,
    );
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
    assert_eq!(
        compile_and_run_tagged(source),
        "rebound|caught|rebind:rebound|caught|rebind",
    );
}
