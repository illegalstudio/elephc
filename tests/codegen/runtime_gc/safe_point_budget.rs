//! Purpose:
//! Checks that automatic unset safe points collect at once on small heaps and defer on large
//! heaps until their saved scan credit covers the heap, while explicit collection stays eager.
//!
//! Called from:
//! - The `codegen_tests` runtime GC suite.
//!
//! Key details:
//! - `gc_status()['runs']` counts only productive passes, so each probe strands one cycle.
//! - The ballast list pushes the heap well past one 64 KiB credit; the unset right after it
//!   collects because live bytes outgrew their limit, which resets both budgets.
//! - Deferred cycles and their destructors are reclaimed by a later safe point, explicitly,
//!   or by the shutdown collection, also in builds without heap debugging.
//! - Garbage that outgrows the reachable heap forces a collection whatever the scan credit.

use crate::support::*;

/// Small heaps collect at every unset; large heaps defer, then reclaim within the budget.
#[test]
fn test_safe_point_budget_defers_on_large_heaps_only() {
    let source = r#"<?php
class Node { public $next; }
class Loud
{
    public $peer;
    public function __construct(public string $name) {}
    public function __destruct() { echo "gone:", $this->name, " "; }
}
function runs(): int { return gc_status()['runs']; }
function strandCycle(): void { $a = new Node(); $b = new Node(); $a->next = $b; $b->next = $a; }
function strandLoud(string $name): void { $a = new Loud($name); $b = new Loud($name); $a->peer = $b; $b->peer = $a; }
$few = [new Node(), new Node()];
strandCycle();
$before = runs();
unset($few[0]);
echo "small=", runs() - $before, "\n";
$nodes = [];
for ($i = 0; $i < $argc + 299; $i++) { $nodes[$i] = new Node(); }
$ballast = [];
for ($i = 0; $i < $argc + 19999; $i++) { $ballast[] = str_repeat('x', 40) . $i; }
unset($few[1]);
strandCycle();
strandLoud('a');
$before = runs();
unset($nodes[1]);
echo "deferred=", runs() - $before, " ";
for ($i = 2; $i < 300; $i++) { unset($nodes[$i]); }
echo "later=", runs() - $before, "\n";
strandLoud('b');
gc_collect_cycles();
echo "explicit ", count($nodes), count($ballast), "\n";
"#;
    let output = compile_and_run_with_heap_debug(source);
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
    assert_eq!(
        output.stdout,
        "small=1\ndeferred=0 gone:a gone:a later=1\ngone:b gone:b explicit 120000\n"
    );
}

/// Removals that each strand far more than one credit keep the heap bounded (phcorp's repro).
#[test]
fn test_safe_point_budget_collects_when_garbage_outgrows_live_bytes() {
    let source = r#"<?php
class G { public $peer; public $payload; }
function make(int $i): G
{
    $a = new G(); $b = new G(); $a->peer = $b; $b->peer = $a;
    $a->payload = str_repeat('x', 200000) . $i;
    return $a;
}
$graphs = [];
for ($i = 0; $i < $argc + 999; $i++) { $graphs[$i] = make($i); unset($graphs[$i]); }
$peak = memory_get_peak_usage();
echo "done ", $peak < 2000000 ? "bounded" : "peak=" . $peak, "\n";
"#;
    let output = compile_and_run_with_heap_debug(source);
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
    assert_eq!(output.stdout, "done bounded\n");
}

/// Without heap debugging, a deferred cycle's destructors still run at the normal script end.
#[test]
fn test_safe_point_budget_shutdown_runs_deferred_destructors() {
    let source = r#"<?php
class Node { public $next; }
class Loud
{
    public $peer;
    public function __construct(public string $name) {}
    public function __destruct() { echo "gone:", $this->name, " "; }
}
function strandLoud(string $name): void { $a = new Loud($name); $b = new Loud($name); $a->peer = $b; $b->peer = $a; }
$ballast = [];
for ($i = 0; $i < $argc + 19999; $i++) { $ballast[] = str_repeat('x', 40) . $i; }
$nodes = [new Node(), new Node(), new Node()];
unset($nodes[0]);
strandLoud('b');
unset($nodes[1]);
echo "deferred ";
strandLoud('c');
echo "end ";
"#;
    let stdout = compile_and_run(source);
    assert!(stdout.starts_with("deferred end "), "{stdout}");
    assert_eq!(stdout.matches("gone:b ").count(), 2, "{stdout}");
    assert_eq!(stdout.matches("gone:c ").count(), 2, "{stdout}");
}

/// A safe point reached while `gc_disable()` is active must not spend the budget or raise the
/// live limit, so the first safe point after `gc_enable()` still collects (Grok review).
#[test]
fn test_safe_point_budget_disabled_collector_keeps_budget() {
    let source = r#"<?php
class G { public $peer; public $payload; }
class Loud
{
    public $peer;
    public function __construct(public string $name) {}
    public function __destruct() { echo "gone:", $this->name, " "; }
}
function strandBig(): void { $a = new G(); $b = new G(); $a->peer = $b; $b->peer = $a; $a->payload = str_repeat('x', 200000); }
function strandLoud(string $name): void { $a = new Loud($name); $b = new Loud($name); $a->peer = $b; $b->peer = $a; }
$ballast = [];
for ($i = 0; $i < $argc + 19999; $i++) { $ballast[] = str_repeat('x', 40) . $i; }
$nodes = [new G(), new G(), new G()];
gc_disable();
strandBig();
unset($nodes[0]);
gc_enable();
strandLoud('x');
unset($nodes[1]);
echo "after ", count($nodes), count($ballast), "\n";
"#;
    let output = compile_and_run_with_heap_debug(source);
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
    assert_eq!(output.stdout, "gone:x gone:x after 120000\n");
}
