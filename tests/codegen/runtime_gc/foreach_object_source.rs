//! Purpose:
//! Regression tests for the reference a `foreach` loop holds on an *object*
//! source — a `Generator`, or a user class implementing `Iterator`. That
//! reference used to be taken by a bare backend `incref` inside `Op::IterStart`
//! that nothing ever balanced, so every such loop leaked the iterated object
//! and every heap block it owned.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Each fixture runs under `--heap-debug` and asserts `leak summary: clean`,
//!   so a reintroduced imbalance fails here instead of silently growing the heap.
//! - The reference is now an `Op::Acquire` released by the loop's exit block and
//!   its `LoopCleanup`, which is also what keeps `unset($it)` inside the body
//!   from freeing the object mid-iteration — hence the `unset` fixtures, which
//!   would regress into a use-after-free if the acquire were simply dropped.
//! - Expected stdout is real `LC_ALL=C php` 8.4 output.

use crate::support::compile_and_run_with_heap_debug;

/// Native SPL iterator results own their cells with heap debugging both enabled and disabled.
#[test]
fn test_spl_foreach_intrinsic_result_owners_with_and_without_heap_debug() {
    let source = r#"<?php
class SplForeachOwnedValue {
    public string $name;
    public function __construct(string $name) { $this->name = $name; }
    public function __destruct() { echo 'drop-', $this->name, '|'; }
}
$list = new SplDoublyLinkedList(); $list->push(new SplForeachOwnedValue('list'));
foreach ($list as $key => $value) { echo $key, ':', $value->name, '|'; }
unset($value, $list); echo 'after-list|';
$stack = new SplStack(); $stack->push(new SplForeachOwnedValue('stack'));
foreach ($stack as $key => $value) { echo $key, ':', $value->name, '|'; }
unset($value, $stack); echo 'after-stack|';
$queue = new SplQueue(); $queue->enqueue(new SplForeachOwnedValue('queue'));
foreach ($queue as $key => $value) { echo $key, ':', $value->name, '|'; }
unset($value, $queue); echo 'after-queue';
"#;
    let expected = "0:list|drop-list|after-list|0:stack|drop-stack|after-stack|0:queue|drop-queue|after-queue";
    assert_eq!(crate::support::compile_and_run(source), expected);
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_clean(out, expected);
}

/// Asserts the program printed `expected` and left a clean heap under heap debug.
fn assert_clean(out: crate::support::ProgramOutput, expected: &str) {
    assert_eq!(out.stdout, expected, "stderr: {}", out.stderr);
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean heap, got: {}",
        out.stderr
    );
}

/// The reported repro: a fully consumed two-value generator must not leak. It
/// used to end the program with `live_blocks=5` — the generator object plus the
/// persistent key/value/return cells it owns, none of which were reclaimed
/// because the generator's refcount never reached zero.
#[test]
fn test_foreach_over_generator_temporary_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function gen() { yield "a"; yield "b"; }
foreach (gen() as $v) { echo $v; }
echo "\n";
"#,
    );
    assert_clean(out, "ab\n");
}

/// The same generator held in a local instead of iterated as a temporary, and
/// iterated twice, so a per-loop imbalance shows up as a multiple.
#[test]
fn test_foreach_over_generator_local_and_repeated_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function gen() { yield 1; }
$g = gen();
foreach ($g as $v) { echo $v; }
foreach (gen() as $v) { echo $v; }
foreach (gen() as $v) { echo $v; }
echo "\n";
"#,
    );
    assert_clean(out, "111\n");
}

/// `break` and an early `return` leave the loop without falling through its
/// exit block, so they exercise the `LoopCleanup` release rather than the
/// exit-block one.
#[test]
fn test_foreach_over_generator_break_and_return_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function gen() { yield 1; yield 2; yield 3; }
foreach (gen() as $v) { echo $v; if ($v == 2) break; }
function first_even() { foreach (gen() as $v) { if ($v % 2 == 0) { return $v; } } return 0; }
echo first_even();
echo "\n";
"#,
    );
    assert_clean(out, "122\n");
}

/// An exception thrown out of the loop body unwinds past both the exit block
/// and the loop cleanup, so the generator and its cells must still be reclaimed.
#[test]
fn test_foreach_over_generator_throw_from_body_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function gen() { yield 1; yield 2; }
try { foreach (gen() as $v) { echo $v; throw new RuntimeException("x"); } }
catch (RuntimeException $e) { echo "C:", $e->getMessage(); }
echo "\n";
"#,
    );
    assert_clean(out, "1C:x\n");
}

/// The same imbalance affected every object source, not only generators: a
/// user class implementing `Iterator` leaked one object per loop.
#[test]
fn test_foreach_over_user_iterator_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class Counter implements Iterator {
    private int $i = 0;
    public function current(): mixed { return $this->i; }
    public function key(): mixed { return $this->i; }
    public function next(): void { $this->i++; }
    public function rewind(): void { $this->i = 0; }
    public function valid(): bool { return $this->i < 3; }
}
$c = new Counter();
foreach ($c as $v) { echo $v; }
foreach (new Counter() as $v) { echo $v; }
echo "\n";
"#,
    );
    assert_clean(out, "012012\n");
}

/// PHP keeps the iterated object alive for the whole loop even when the body
/// drops every other owner, so dropping the loop's own reference instead of
/// balancing it would turn this fixture into a use-after-free that stops after
/// one iteration. Covers both `unset()` and a plain rebind.
#[test]
fn test_foreach_body_dropping_source_variable_keeps_iterating() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class Counter implements Iterator {
    private int $i = 0;
    public function current(): mixed { return $this->i; }
    public function key(): mixed { return $this->i; }
    public function next(): void { $this->i++; }
    public function rewind(): void { $this->i = 0; }
    public function valid(): bool { return $this->i < 3; }
}
function gen() { yield 1; yield 2; }
$a = new Counter();
foreach ($a as $v) { echo $v; unset($a); }
$b = new Counter();
foreach ($b as $v) { echo $v; $b = null; }
$g = gen();
foreach ($g as $v) { echo $v; unset($g); }
echo "\n";
"#,
    );
    assert_clean(out, "01201212\n");
}
