//! Purpose:
//! Checks owned temporaries and native activations across coroutine suspension and close.
//!
//! Called from:
//! - Focused runtime GC codegen regressions.
//!
//! Key details:
//! - Delegation preserves PHP finally order and shared inner generator lifetimes.
//! - Fresh start argument boxes and cleanup exception chains remain balanced.

use crate::support::*;

/// Throwing cleanup cannot expose the private cancellation marker as a previous exception.
#[test]
fn test_gc_coroutine_boundary_hides_marker_from_destructor_cleanup() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class BoundaryThrower { public function __destruct() { throw new Exception('dtor'); } }
function boundaryRun(callable $take): void { $take(new BoundaryThrower(), Fiber::suspend()); }
$fiber = new Fiber(function (): void { boundaryRun(function ($a, $b): void {}); });
$fiber->start();
try { unset($fiber); }
catch (Exception $error) { echo $error->getMessage(), ':', $error->getPrevious() === null ? 'none|' : 'leaked|'; }
class BoundarySuspendingDestructor {
    public function __destruct() { $value = new BoundaryThrower(); Fiber::suspend(); }
}
$other = new Fiber(function (): void { $value = new BoundarySuspendingDestructor(); unset($value); });
$other->start();
try { unset($other); }
catch (Exception $error) { echo $error->getMessage(), ':', $error->getPrevious() === null ? 'none' : 'leaked'; }
"#);
    assert!(out.success, "stdout: {}\nstderr: {}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "dtor:none|dtor:none", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Fresh delegation closes first while local and external aliases retain their PHP lifetime.
#[test]
fn test_gc_coroutine_boundary_closes_delegated_generator_owners() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function boundaryInner() { try { yield 1; yield 2; } finally { echo 'inner|'; } }
function boundaryOuter() { try { yield from boundaryInner(); } finally { echo 'outer|'; } }
$first = boundaryOuter(); $first->current(); unset($first); echo 'done|';
$second = boundaryOuter(); $second->current(); $second = null; gc_collect_cycles(); echo 'done2|';
function boundaryLocal() {
    $local = boundaryInner();
    try { yield from $local; } finally { echo 'local|'; }
}
$third = boundaryLocal(); $third->current(); unset($third); echo 'done3|';
function boundaryShared(Generator $inner) {
    try { yield from $inner; } finally { echo 'shared|'; }
}
$inner = boundaryInner(); $fourth = boundaryShared($inner); $fourth->current();
unset($fourth); echo 'retained|'; unset($inner); echo 'done4';
"#);
    assert!(out.success, "stdout: {}\nstderr: {}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "inner|outer|done|inner|outer|done2|local|inner|done3|shared|retained|inner|done4", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Start boxes retire after callbacks acquire parameters, including resume and finally reads.
#[test]
fn test_gc_coroutine_boundary_releases_fresh_start_argument_boxes() {
    let out = compile_and_run_with_heap_debug(r#"<?php
for ($iteration = 0; $iteration < 8; $iteration++) {
    $normal = new Fiber(static function(string $value): void { echo strlen($value), '|'; });
    $normal->start(str_repeat('a', 4096));
    try { $normal->start(str_repeat('b', 4096)); } catch (FiberError $error) {}
    unset($normal);
    $suspended = new Fiber(static function(string $value): void {
        try { Fiber::suspend(); } finally { echo strlen($value), '|'; }
    });
    $suspended->start(str_repeat('c', 4096)); unset($suspended);
    $variadic = new Fiber(static function(mixed ...$values): void {
        Fiber::suspend(); echo strlen($values[0]), '|';
    });
    $variadic->start(str_repeat('d', 4096)); $variadic->resume(); unset($variadic);
}
$failing = new Fiber(static function(string $value): void { throw new Exception('failed'); });
try { $failing->start(str_repeat('e', 4096)); } catch (Exception $error) {}
unset($failing);
"#);
    assert!(out.success, "stdout: {}\nstderr: {}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "4096|4096|4096|".repeat(8), "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A warning callback can suspend and collect with a complete native owner activation.
#[test]
fn test_gc_coroutine_boundary_enumerates_suspended_warning_frames() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function boundaryDepth(int $n): int { return $n === 0 ? 0 : 1 + boundaryDepth($n - 1); }
set_error_handler(function (int $level, string $message): bool { Fiber::suspend(); return true; });
for ($iteration = 0; $iteration < 4; $iteration++) {
    $fiber = new Fiber(function (): void {
        boundaryDepth(200);
        $values = []; $missing = $values['missing']; echo 'resumed|';
    });
    $fiber->start(); gc_collect_cycles(); $fiber->resume(); unset($fiber);
}

restore_error_handler();
echo 'done';
"#);
    assert!(out.success, "stdout: {}\nstderr: {}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "resumed|resumed|resumed|resumed|done", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Fresh warning activations overwrite poisoned stack words before suspended GC visits them.
#[test]
fn test_gc_coroutine_boundary_visits_warning_on_reused_native_stack() {
    let out = compile_and_run_with_c_fixture(r#"<?php
extern function boundary_poison_stack(): void;
set_error_handler(function (int $level, string $message): bool { Fiber::suspend(); return true; });
$fiber = new Fiber(static function(): void {
    boundary_poison_stack();
    $values = []; $missing = $values['missing']; echo 'resumed|';
});
$fiber->start(); gc_collect_cycles(); $fiber->resume(); unset($fiber);
restore_error_handler(); echo 'done';
"#, r#"
__attribute__((noinline)) void boundary_poison_stack(void) {
    volatile unsigned long slots[2048];
    for (unsigned long i = 0; i < 2048; ++i) slots[i] = 1;
    (void)slots[0];
}
"#);
    assert_eq!(out, "resumed|done");
}

/// Nested includes keep one activation each inside the including function's frame. Spaced tighter
/// than a record, the inner record's visitor word was the outer record's previous link, and the
/// collector jumped through it.
#[test]
fn test_gc_coroutine_boundary_skips_nested_include_activations() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
function run(): void { include_once 'outer.php'; }
$fiber = new Fiber('run');
$fiber->start(); gc_collect_cycles(); $fiber->resume(); unset($fiber);
echo 'done';
"#,
            ),
            ("outer.php", "<?php include_once 'inner.php'; $trace = debug_backtrace();"),
            ("inner.php", "<?php Fiber::suspend(); echo 'resumed|';"),
        ],
        "main.php",
    );
    assert_eq!(out, "resumed|done");
}

/// Both exceptional suspension exits notify the monitoring callback with the same coroutine.
#[test]
fn test_gc_coroutine_boundary_unparks_rejected_forced_suspension() {
    let out = compile_and_run_with_c_fixture(r#"<?php
extern function boundary_install_unpark(): void;
extern function boundary_unpark_count(): int;
boundary_install_unpark();
$fiber = new Fiber(static function(): void {
    try { Fiber::suspend(); } finally { Fiber::suspend(); }
});
$fiber->start();
try { unset($fiber); } catch (FiberError $error) { echo 'caught|'; }
echo boundary_unpark_count();
"#, r#"
extern void (*elephc_instr_unpark_fn)(unsigned long, unsigned long, void *);
static void *coroutine;
static long count;
static void boundary_unpark(unsigned long allocations, unsigned long frees, void *current) {
    (void)allocations; (void)frees;
    if (current && (!coroutine || current == coroutine)) ++count;
    else count = -100;
    coroutine = current;
}
void boundary_install_unpark(void) { elephc_instr_unpark_fn = boundary_unpark; }
long boundary_unpark_count(void) { return count; }
"#);
    assert_eq!(out, "caught|2");
}

/// A suspended generator or fiber is the only holder of self-referencing objects, through an
/// object local, an array local and a by-reference alias: a collection while they are suspended
/// must free none of them, and the cycles are freed once the coroutines are gone. The expected
/// output is php 8.5's own.
#[test]
fn test_gc_suspended_frame_owner_tables_keep_live_cycles() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class Node { public $next; public $tag; function __construct($tag) { $this->tag = $tag; } }
function holder() {
    $a = new Node('a');
    $a->next = $a;
    $list = [new Node('l')];
    $list[0]->next = $list[0];
    $r = new Node('r');
    $r->next = $r;
    $alias = &$r;
    $box = $argc ?? null;
    yield 1;
    echo $a->next->tag, $list[0]->next->tag, $alias->next->tag, "\n";
    yield 2;
}
$g = holder();
$g->current();
echo gc_collect_cycles() >= 0 ? "collected\n" : "?\n";
$g->next();
echo $g->current(), "\n";
$f = new Fiber(function () {
    $n = new Node('f');
    $n->next = $n;
    $ref = &$n;
    Fiber::suspend();
    echo $ref->next->tag, "\n";
});
$f->start();
gc_collect_cycles();
$f->resume();
unset($g, $f);
echo gc_collect_cycles() > 0 ? "freed\n" : "none\n";
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "collected\nalr\n2\nf\nfreed\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
