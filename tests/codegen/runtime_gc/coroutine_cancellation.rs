//! Purpose:
//! Checks forced coroutine destruction and typed suspended-frame ownership.
//!
//! Called from:
//! - Focused CLI runtime ownership regressions.
//!
//! Key details:
//! - PHP catch clauses must not intercept internal force-close unwinding.
//! - Finally blocks run before stacks disappear, including real exceptional cleanup.
//! - Suspended frame cycles must not appear to have external owners.

use crate::support::*;

/// Finally mutations can rescue a cyclic owner before the collector's post-destructor recount.
#[test]
fn test_gc_coroutine_cancellation_recounts_finally_resurrection() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class RescuedCoroutineFrame {
    public static ?RescuedCoroutineFrame $saved = null;
    public static int $destroyed = 0;
    public $iterator = null;
    public string $payload;
    public function __construct() {
        $this->payload = str_repeat('x', 4096);
        $generator = $this->rows();
        $generator->current();
        $this->iterator = $generator;
    }
    public function rows() {
        $owners = [$this];
        try { yield 1; yield count($owners); }
        finally { self::$saved = $this; }
    }
    public function __destruct() { self::$destroyed++; }
}
function rescuedFrame(): WeakReference {
    $owner = new RescuedCoroutineFrame();
    return WeakReference::create($owner);
}
$weak = rescuedFrame();
gc_collect_cycles();
echo RescuedCoroutineFrame::$destroyed, ':', strlen(RescuedCoroutineFrame::$saved->payload), ':', $weak->get() === null ? 'dead|' : 'alive|';
RescuedCoroutineFrame::$saved = null;
gc_collect_cycles();
echo RescuedCoroutineFrame::$destroyed, ':', $weak->get() === null ? 'dead' : 'alive';
unset($weak);
"#);
    assert!(out.success, "stdout: {}\nstderr: {}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "1:4096:alive|1:dead", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Other Fibers can run during finally while nested abandoned frames still unwind.
#[test]
fn test_gc_coroutine_cancellation_allows_php_switches_and_unwinds_nested_fibers() {
    let out = compile_and_run_with_heap_debug(r#"<?php
$fiber = new Fiber(static function(): void {
    $nested = new Fiber(static function(): void {
        try { Fiber::suspend(); } finally { echo 'nested-closed|'; }
    });
    $nested->start();
    try { Fiber::suspend(); }
    finally {
        $other = new Fiber(static function(): void { echo 'other-started|'; });
        try { $other->start(); }
        catch (FiberError $error) { echo $error->getMessage(), '|'; }
        echo $other->isStarted() ? 'started|' : 'new|';
        unset($nested);
    }
});
$fiber->start();
unset($fiber);
echo 'done';
"#);
    assert!(out.success, "stdout: {}\nstderr: {}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "other-started|started|nested-closed|done", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Managed references and temporary argument guards preserve an externally retained frame cycle.
#[test]
fn test_gc_coroutine_cancellation_preserves_shared_reference_and_argument_owners() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class ReferenceFrameOwner {
    public static int $destroyed = 0;
    public ?Fiber $fiber = null;
    public string $payload;
    public function __construct() { $this->payload = str_repeat('x', 4096); }
    public function __destruct() { self::$destroyed++; }
}
class ReferenceFrameTemporary {
    public static int $destroyed = 0;
    public function __destruct() { self::$destroyed++; }
}
function suspendWithReference(ReferenceFrameOwner &$owner, ReferenceFrameTemporary $temporary): void {
    $owners = [$owner];
    try { Fiber::suspend(); echo count($owners); }
    finally { echo 'closed|'; }
}
function exerciseReferenceFrame(mixed $owner): WeakReference {
$weak = WeakReference::create($owner);
$owner->fiber = new Fiber(static function() use (&$owner): void {
    $alias =& $owner;
    suspendWithReference($alias, new ReferenceFrameTemporary());
});
$owner->fiber->start();
for ($index = 0; $index < 8; $index++) { gc_collect_cycles(); }
echo strlen($owner->payload), ':', ReferenceFrameOwner::$destroyed, ':', ReferenceFrameTemporary::$destroyed, '|';
unset($owner);
return $weak;
}
$weak = exerciseReferenceFrame(new ReferenceFrameOwner());
gc_collect_cycles();
echo ReferenceFrameOwner::$destroyed, ':', ReferenceFrameTemporary::$destroyed, ':', $weak->get() === null ? 'dead' : 'alive';
unset($weak);
"#);
    assert!(out.success, "stdout: {}\nstderr: {}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "4096:0:0|closed|1:1:dead", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Abandoned Generator and Fiber frames destroy acyclic locals and execute finally exactly once.
#[test]
fn test_gc_coroutine_cancellation_releases_locals_and_runs_finally() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class CancellationLocal {
    public static int $destroyed = 0;
    public string $payload;
    public function __construct() { $this->payload = str_repeat('x', 4096); }
    public function __destruct() { self::$destroyed++; }
}
function cancelGenerator() {
    $local = new CancellationLocal();
    try { yield 1; yield 2; }
    catch (Throwable $error) { echo 'caught-marker|'; }
    finally { echo 'generator-finally|'; }
}
$generator = cancelGenerator();
$generator->current();
unset($generator);
echo CancellationLocal::$destroyed, '|';
$fiber = new Fiber(static function(): void {
    $local = new CancellationLocal();
    try { Fiber::suspend(1); }
    catch (Throwable $error) { echo 'caught-marker|'; }
    finally { echo 'fiber-finally|'; }
});
$fiber->start();
unset($fiber);
echo CancellationLocal::$destroyed;
"#);
    assert!(out.success, "stdout: {}\nstderr: {}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "generator-finally|1|fiber-finally|2", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Real finally failures propagate while forbidden suspension has PHP's Error subclasses.
#[test]
fn test_gc_coroutine_cancellation_preserves_real_errors_and_hides_markers() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function realCancellationError() { try { yield 1; } finally { throw new RuntimeException('finally failed'); } }
$generator = realCancellationError();
$generator->current();
try { unset($generator); } catch (RuntimeException $error) {
    echo $error->getMessage(), ':', $error->getPrevious() === null ? 'none|' : 'marker-leaked|';
}
function invalidCancellationYield() { try { yield 1; } finally { yield 2; } }
$generator = invalidCancellationYield();
$generator->current();
try { unset($generator); } catch (Error $error) { echo get_class($error), ':', $error->getMessage(), '|'; }
$fiber = new Fiber(static function(): void { try { Fiber::suspend(); } finally { Fiber::suspend(); } });
$fiber->start();
try { unset($fiber); } catch (FiberError $error) { echo get_class($error), ':', $error->getMessage(); }
unset($error);
"#);
    assert!(out.success, "stdout: {}\nstderr: {}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "finally failed:none|Error:Cannot yield from finally in a force-closed generator|FiberError:Cannot suspend in a force-closed fiber", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Owned arrays in suspended frames close cycles while external coroutine roots stay valid.
#[test]
fn test_gc_coroutine_cancellation_collects_suspended_frame_cycles() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class SuspendedCycle {
    public static int $destroyed = 0;
    public $iterator = null;
    public function __construct() {
        $generator = $this->rows();
        $generator->current();
        $this->iterator = $generator;
    }
    public function rows() {
        $owners = [$this];
        try { yield 1; yield count($owners); }
        finally { echo 'closed|'; }
    }
    public function __destruct() { self::$destroyed++; }
}
$owner = new SuspendedCycle();
$weak = WeakReference::create($owner);
gc_collect_cycles();
echo SuspendedCycle::$destroyed, ':', $weak->get() === null ? 'dead|' : 'alive|';
unset($owner);
gc_collect_cycles();
echo SuspendedCycle::$destroyed, ':', $weak->get() === null ? 'dead' : 'alive';
unset($weak);
"#);
    assert!(out.success, "stdout: {}\nstderr: {}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "0:alive|closed|1:dead", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
