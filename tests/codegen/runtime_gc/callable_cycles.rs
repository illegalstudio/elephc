//! Purpose:
//! Verifies structural cycle ownership through native callable capture descriptors.
//!
//! Called from:
//! - Focused CLI runtime GC regression tests.
//!
//! Key details:
//! - Portable fixtures cover captures, bound receivers, nested descriptors and Fiber owners.
//! - Explicit collection must preserve externally owned callbacks and reclaim detached cycles.

use crate::support::*;

const HOLDERS: &str = include_str!("../../fixtures/callable_cycles.php");

/// Repeated automatic collection visits many reference captures without walking the heap per edge.
#[test]
fn test_gc_callable_reference_captures_survive_repeated_automatic_collection() {
    let out = compile_and_run(r#"<?php
function makeCapturedReference(int $value): Closure {
    return static function() use (&$value): int { return $value; };
}
$callbacks = [];
for ($index = 0; $index < 3000; $index++) { $callbacks[] = makeCapturedReference($index); }
for ($index = 0; $index < 100; $index++) { $temporary = [$index]; unset($temporary); }
$callback = $callbacks[2999];
echo count($callbacks), ':', $callback();
"#);
    assert_eq!(out, "3000:2999");
}

/// Every native capture shape participates in incoming counts, marking and final reclamation.
#[test]
fn test_gc_callable_cycles_release_native_capture_owners() {
    let source = format!(r#"{HOLDERS}
function makeCycle(string $kind): WeakReference {{
    $holder = new CapturedCycleHolder($kind);
    echo $holder->invoke(), ':';
    return WeakReference::create($holder);
}}
$weak = [];
foreach (['/closure', '/arrow', '/value', '/reference', '/nested', '/bound', '/method', '/fiber', '/array', '/hash', '/boxed', '/ref-callable'] as $kind) {{
    $weak[] = makeCycle($kind);
}}
gc_collect_cycles();
echo CapturedCycleHolder::$destroyed, ':';
foreach ($weak as $observer) {{ echo $observer->get() === null ? 'dead|' : 'alive|'; }}
unset($observer, $weak);
gc_collect_cycles();
"#);
    let out = compile_and_run_with_heap_debug(&source);
    assert!(out.success, "stdout: {}\nstderr: {}", out.stdout, out.stderr);
    assert_eq!(out.stdout, format!("{}12:{}", "4096:".repeat(12), "dead|".repeat(12)), "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A rooted closure keeps its capture valid; throwing and resurrecting cycles balance teardown.
#[test]
fn test_gc_callable_cycles_preserve_roots_and_destructor_changes() {
    let source = format!(r#"{HOLDERS}
function callbackOwner(): callable {{
    $holder = new CapturedCycleHolder('/reference');
    return static function() use ($holder): void {{ echo strlen($holder->payload); }};
}}
$read = callbackOwner();
gc_collect_cycles();
$read();
echo ':', CapturedCycleHolder::$destroyed, '|';
unset($read);
gc_collect_cycles();
echo CapturedCycleHolder::$destroyed, '|';
$holder = new CapturedCycleHolder('/throw');
$weak = WeakReference::create($holder);
try {{ unset($holder); gc_collect_cycles(); }} catch (RuntimeException $error) {{ echo $error->getMessage(), '|'; }}
echo CapturedCycleHolder::$destroyed, ':', $weak->get() === null ? 'dead|' : 'alive|';
unset($weak, $error);
$holder = new CapturedCycleHolder('/resurrect');
$weak = WeakReference::create($holder);
unset($holder);
gc_collect_cycles();
echo CapturedCycleHolder::$destroyed, ':', CapturedCycleHolder::$saved->invoke(), '|';
CapturedCycleHolder::$saved = null;
gc_collect_cycles();
echo $weak->get() === null ? 'dead|' : 'alive|';
unset($weak);
$holder = new CapturedCycleHolder('/mutate');
unset($holder);
gc_collect_cycles();
echo CapturedCycleHolder::$destroyed;
"#);
    let out = compile_and_run_with_heap_debug(&source);
    assert!(out.success, "stdout: {}\nstderr: {}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "4096:0|1|captured-cycle cleanup|2:dead|3:4096|dead|4", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Saturated object incoming counts include native descriptor captures during the exact rescan.
#[test]
fn test_gc_callable_cycles_recount_saturated_native_captures() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class SaturatedCapturedOwner {
    public static int $destroyed = 0;
    public array $callbacks = [];
    public function __construct() {
        $owner = $this;
        for ($i = 0; $i < 4100; $i++) { $this->callbacks[] = static fn() => $owner; }
    }
    public function __destruct() { self::$destroyed++; }
}
$owner = new SaturatedCapturedOwner();
$weak = WeakReference::create($owner);
unset($owner);
gc_collect_cycles();
echo SaturatedCapturedOwner::$destroyed, ':', $weak->get() === null ? 'dead' : 'alive';
unset($weak);
"#);
    assert!(out.success, "stdout: {}\nstderr: {}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "1:dead", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
