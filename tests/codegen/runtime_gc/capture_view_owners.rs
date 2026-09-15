//! Purpose:
//! Regressions for by-value closure and Fiber captures that outlive their source locals.
//!
//! Called from:
//! - The runtime GC codegen integration suite.
//!
//! Key details:
//! - Destructor order and heap diagnostics distinguish a valid retained capture from
//!   both a dangling borrowed view and a leaked owning conversion.

use crate::support::*;

/// A by-value callable capture stays valid after its source variable is cleared.
///
/// `$callable` is a callable slot read as a callable: a borrowed view. The descriptor retains
/// it, so releasing that view too makes `$callable = null` free the descriptor the closure
/// still points at. The destructor output pins WHEN the wrapped object dies, after the
/// invocation and only when the closure itself is retired.
#[test]
fn test_core_closure_callable_capture_survives_its_cleared_source() {
    let source = r#"<?php
class Tracked {
    public function __construct(public string $tag) {}
    public function render(): string { return 'r:' . $this->tag; }
    public function __destruct() { echo 'gone:', $this->tag, '|'; }
}
function callableCaptureSurvivesClearedSource(): string {
    $tracked = new Tracked('one');
    $callable = $tracked->render(...);
    $closure = function () use ($callable): string { return $callable(); };
    $callable = null;
    $tracked = null;
    echo 'invoke|';
    echo $closure(), '|';
    unset($closure);
    echo 'retired|';
    return 'ok';
}
echo callableCaptureSurvivesClearedSource();
"#;
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "invoke|r:one|gone:one|retired|ok", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
    assert_eq!(compile_and_run_tagged(source), "invoke|r:one|gone:one|retired|ok");
}

/// A Fiber closure descriptor keeps its callable capture after the source variable is cleared.
///
/// The Fiber holds the closure descriptor, which holds the captured first-class callable. The
/// capture view is borrowed, so an over-release makes `$callable = null` free the descriptor
/// the fiber body invokes. Retiring the fiber explicitly is what fixes the destructor's
/// position in the output rather than leaving it to frame teardown.
#[test]
fn test_core_fiber_closure_callable_capture_survives_its_cleared_source() {
    let source = r#"<?php
class Tracked {
    public function __construct(public string $tag) {}
    public function render(): string { return 'r:' . $this->tag; }
    public function __destruct() { echo 'gone:', $this->tag, '|'; }
}
function fiberCallableCaptureSurvivesClearedSource(): string {
    $tracked = new Tracked('two');
    $callable = $tracked->render(...);
    $fiber = new Fiber(function () use ($callable): void { echo $callable(), '|'; });
    $callable = null;
    $tracked = null;
    echo 'start|';
    $fiber->start();
    unset($fiber);
    echo 'retired|';
    return 'ok';
}
echo fiberCallableCaptureSurvivesClearedSource();
"#;
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "start|r:two|gone:two|retired|ok", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
    assert_eq!(compile_and_run_tagged(source), "start|r:two|gone:two|retired|ok");
}

/// By-value captures snapshot Mixed integer and boxed array sources, not their later values.
///
/// `$counter` is Mixed because `$seed + 3` can overflow to float, so its capture is a borrowed
/// Mixed view; over-releasing it lets `$counter = 99` free the shared cell and the closure then
/// reads reused storage. `$items` is read as a concrete array from a slot the by-reference call
/// widens to PHP `array`, so its unbox-and-retain release must survive or the replaced payload
/// is freed underneath the descriptor. Both calls run so a per-call imbalance shows up.
#[test]
fn test_core_by_value_captures_snapshot_mixed_int_and_array_sources() {
    let source = r#"<?php
function dropCaptureArray(array &$slot): void { $slot = ['changed']; }
function snapshotMixedCaptures(int $seed): string {
    $counter = $seed + 3;
    $items = ['kept'];
    $snapshot = function () use ($counter, $items): string {
        return $counter . ':' . $items[0];
    };
    $counter = 99;
    dropCaptureArray($items);
    return $snapshot();
}
echo snapshotMixedCaptures($argc), '|', snapshotMixedCaptures($argc);
"#;
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "4:kept|4:kept", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
    assert_eq!(compile_and_run_tagged(source), "4:kept|4:kept");
}
