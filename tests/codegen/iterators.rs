//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of iterators, including foreach user iterator value only, foreach user iterator with key, and foreach user iterator break.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use crate::support::*;

/// Tests a user-defined class implementing Iterator with value-only foreach syntax.
/// The foreach dispatches rewind/valid/current/next against the concrete class,
/// emitting only the value ($v) on each iteration.
#[test]
fn test_foreach_user_iterator_value_only() {
    let out = compile_and_run(
        r#"<?php
class Range implements Iterator {
    private int $i;
    private int $end;
    public function __construct(int $start, int $end) {
        $this->i = $start;
        $this->end = $end;
    }
    public function rewind(): void {}
    public function valid(): bool { return $this->i < $this->end; }
    public function current(): mixed { return $this->i; }
    public function key(): mixed { return $this->i; }
    public function next(): void { $this->i = $this->i + 1; }
}
foreach (new Range(0, 3) as $v) { echo $v; }
"#,
    );
    assert_eq!(out, "012");
}

/// Tests a user-defined class implementing Iterator with key=>value foreach syntax.
/// Verifies both $k and $v are emitted correctly per iteration.
#[test]
fn test_foreach_user_iterator_with_key() {
    let out = compile_and_run(
        r#"<?php
class IntPair implements Iterator {
    private int $i;
    public function __construct() { $this->i = 10; }
    public function rewind(): void {}
    public function valid(): bool { return $this->i < 13; }
    public function current(): mixed { return $this->i; }
    public function key(): mixed { return $this->i - 10; }
    public function next(): void { $this->i = $this->i + 1; }
}
foreach (new IntPair() as $k => $v) { echo $k; echo ":"; echo $v; echo " "; }
"#,
    );
    assert_eq!(out, "0:10 1:11 2:12 ");
}

/// Tests that break inside a foreach over a user-defined Iterator terminates the loop
/// at the correct iteration (before the iteration where $v == 4).
#[test]
fn test_foreach_user_iterator_break() {
    let out = compile_and_run(
        r#"<?php
class Counter implements Iterator {
    private int $i;
    public function __construct() { $this->i = 0; }
    public function rewind(): void {}
    public function valid(): bool { return true; }
    public function current(): mixed { return $this->i; }
    public function key(): mixed { return $this->i; }
    public function next(): void { $this->i = $this->i + 1; }
}
foreach (new Counter() as $v) {
    if ($v == 4) { break; }
    echo $v;
}
"#,
    );
    assert_eq!(out, "0123");
}

/// Tests a class implementing IteratorAggregate (not Iterator directly). The foreach
/// must call getIterator() once and then dispatch rewind/valid/current/next against
/// the returned object for each iteration.
#[test]
fn test_foreach_iterator_aggregate_class() {
    // A class that implements only IteratorAggregate (not Iterator
    // directly) — foreach calls getIterator() once before the loop and
    // dispatches the per-iteration calls against the returned class.
    let out = compile_and_run(
        r#"<?php
class Range implements Iterator {
    private int $current;
    private int $end;
    public function __construct(int $start, int $end) {
        $this->current = $start;
        $this->end = $end;
    }
    public function rewind(): void {}
    public function valid(): bool { return $this->current < $this->end; }
    public function current(): mixed { return $this->current; }
    public function key(): mixed { return $this->current; }
    public function next(): void { $this->current = $this->current + 1; }
}
class Aggregate implements IteratorAggregate {
    public function getIterator(): Range { return new Range(0, 5); }
}
foreach (new Aggregate() as $v) { echo $v; echo " "; }
"#,
    );
    assert_eq!(out, "0 1 2 3 4 ");
}

/// Tests IteratorAggregate where getIterator() is declared to return the Iterator
/// interface type (而非 a concrete class). The foreach must still dispatch correctly
/// against the concrete Range object returned at runtime.
#[test]
fn test_foreach_iterator_aggregate_returning_iterator_interface() {
    let out = compile_and_run(
        r#"<?php
class Range implements Iterator {
    private int $current;
    private int $end;
    public function __construct(int $start, int $end) {
        $this->current = $start;
        $this->end = $end;
    }
    public function rewind(): void {}
    public function valid(): bool { return $this->current < $this->end; }
    public function current(): int { return $this->current; }
    public function key(): int { return $this->current; }
    public function next(): void { $this->current = $this->current + 1; }
}
class Aggregate implements IteratorAggregate {
    public function getIterator(): Iterator { return new Range(0, 3); }
}
foreach (new Aggregate() as $v) { echo $v; }
"#,
    );
    assert_eq!(out, "012");
}

/// Tests foreach with a function parameter typed as Iterator (interface). Verifies
/// dispatch correctly calls rewind/valid/current/key/next on the concrete Range
/// object passed at runtime and that key() result is used for $k.
#[test]
fn test_foreach_iterator_typed_parameter_dispatches_by_interface() {
    let out = compile_and_run(
        r#"<?php
class Range implements Iterator {
    private int $current;
    private int $end;
    public function __construct(int $start, int $end) {
        $this->current = $start;
        $this->end = $end;
    }
    public function rewind(): void {}
    public function valid(): bool { return $this->current < $this->end; }
    public function current(): int { return $this->current; }
    public function key(): int { return $this->current - 2; }
    public function next(): void { $this->current = $this->current + 1; }
}
function dump_values(Iterator $it): void {
    foreach ($it as $k => $v) {
        echo $k;
        echo "=";
        echo $v;
        echo " ";
    }
}
dump_values(new Range(2, 5));
"#,
    );
    assert_eq!(out, "0=2 1=3 2=4 ");
}

/// Tests that the iterator variable $it can be reused as the foreach value variable
/// ($it again). After the loop, the original iterator reference must remain valid.
#[test]
fn test_foreach_iterator_value_can_reuse_receiver_variable() {
    let out = compile_and_run(
        r#"<?php
class Range implements Iterator {
    private int $current;
    private int $end;
    public function __construct(int $start, int $end) {
        $this->current = $start;
        $this->end = $end;
    }
    public function rewind(): void {}
    public function valid(): bool { return $this->current < $this->end; }
    public function current(): int { return $this->current; }
    public function key(): int { return $this->current; }
    public function next(): void { $this->current = $this->current + 1; }
}
$it = new Range(0, 3);
foreach ($it as $it) {
    echo $it;
}
"#,
    );
    assert_eq!(out, "012");
}

/// Tests that when a function parameter typed as Iterator is consumed by foreach,
/// the parameter variable can be reused as the foreach value variable inside the loop.
#[test]
fn test_foreach_iterator_typed_parameter_can_reuse_receiver_variable() {
    let out = compile_and_run(
        r#"<?php
class Range implements Iterator {
    private int $current;
    private int $end;
    public function __construct(int $start, int $end) {
        $this->current = $start;
        $this->end = $end;
    }
    public function rewind(): void {}
    public function valid(): bool { return $this->current < $this->end; }
    public function current(): int { return $this->current; }
    public function key(): int { return $this->current; }
    public function next(): void { $this->current = $this->current + 1; }
}
function consume(Iterator $it): void {
    foreach ($it as $it) {
        echo $it;
    }
}
consume(new Range(0, 3));
"#,
    );
    assert_eq!(out, "012");
}

/// Tests that pre-existing $k and $v variables outside a foreach are preserved
/// unchanged after iterating over an empty iterator (valid() returns false immediately).
#[test]
fn test_empty_iterator_preserves_existing_key_and_value_variables() {
    let out = compile_and_run(
        r#"<?php
class EmptyIteratorImpl implements Iterator {
    public function rewind(): void {}
    public function valid(): bool { return false; }
    public function current(): int { return 1; }
    public function key(): int { return 2; }
    public function next(): void {}
}
$k = 'key';
$v = 'old';
foreach (new EmptyIteratorImpl() as $k => $v) {
}
echo $k;
echo ':';
echo $v;
"#,
    );
    assert_eq!(out, "key:old");
}

/// Tests that when the iterator variable is reused as the foreach value variable,
/// is_iterable() still returns true on the original iterator object after an empty
/// iteration (the iterator is not consumed by the empty loop).
#[test]
fn test_empty_iterator_preserves_receiver_variable_when_reused_as_value() {
    let out = compile_and_run(
        r#"<?php
class EmptyIteratorImpl implements Iterator {
    public function rewind(): void {}
    public function valid(): bool { return false; }
    public function current(): int { return 1; }
    public function key(): int { return 2; }
    public function next(): void {}
}
$it = new EmptyIteratorImpl();
foreach ($it as $it) {
}
echo is_iterable($it) ? 'iterable' : 'lost';
"#,
    );
    assert_eq!(out, "iterable");
}

/// Tests that fresh $k and $v variables declared in a function scope are initialized
/// to null after a foreach over an empty iterator (not left uninitialized).
#[test]
fn test_empty_iterator_initializes_fresh_function_loop_variables_as_null() {
    let out = compile_and_run(
        r#"<?php
class EmptyIteratorImpl implements Iterator {
    public function rewind(): void {}
    public function valid(): bool { return false; }
    public function current(): int { return 1; }
    public function key(): int { return 2; }
    public function next(): void {}
}
function probe(): void {
    foreach (new EmptyIteratorImpl() as $k => $v) {
    }
    echo is_null($k) ? 'null' : 'key';
    echo ':';
    echo is_null($v) ? 'null' : 'value';
}
probe();
"#,
    );
    assert_eq!(out, "null:null");
}
