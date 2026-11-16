//! Purpose:
//! End-to-end tests for a COMPILED `foreach` over an object an `eval()`-declared class built:
//! the loop must drive the interpreted iterator one step at a time, as php does.
//!
//! Called from:
//! - `cargo test --test codegen_tests eval_compiled_foreach` through Rust's test harness.
//!
//! Key details:
//! - The compiled loop used to drain the interpreted `Traversable` into an `ArrayIterator` before
//!   its first iteration: every `valid()`/`current()`/`next()` ran ahead of the loop body, a
//!   `break` stopped nothing, and an endless generator never let the loop start.
//! - `key()` echoes nothing on purpose: the compiled loop reads the key before the value, which
//!   is not what these tests pin.
//! - Expected output is `php -n` 8.5's.

use crate::support::*;

/// Verifies the iterator protocol interleaves with the loop body, `break` stops the walk, and an
/// aggregate whose `getIterator()` returns another interpreted aggregate is followed to its
/// iterator. The heap must come back clean: the loop owns every iterator it resolved.
#[test]
fn test_compiled_foreach_drives_an_interpreted_iterator_lazily() {
    let output = compile_and_run_with_heap_debug(
        r#"<?php
eval('class LazyIt implements Iterator {
    private $i = 0;
    public function rewind(): void { echo "rewind\n"; $this->i = 0; }
    public function valid(): bool { echo "valid\n"; return $this->i < 3; }
    public function current(): mixed { echo "current\n"; return $this->i * 10; }
    public function key(): mixed { return $this->i; }
    public function next(): void { echo "next\n"; $this->i = $this->i + 1; }
}
class Inner implements IteratorAggregate {
    public function getIterator(): Iterator { return new ArrayIterator(["p" => 1, "q" => 2]); }
}
class Outer implements IteratorAggregate {
    public function getIterator(): Traversable { echo "outer\n"; return new Inner(); }
}');
function make(string $class): mixed { return new $class(); }
function run(): void {
    foreach (make('LazyIt') as $k => $v) { echo "body $k=$v\n"; if ($k === 1) { break; } }
    foreach (make('Outer') as $k => $v) { echo "body $k=$v\n"; }
}
run();
echo "done\n";
"#,
    );
    assert!(output.success, "stdout: {}\nstderr: {}", output.stdout, output.stderr);
    assert_eq!(
        output.stdout,
        "rewind\nvalid\ncurrent\nbody 0=0\nnext\nvalid\ncurrent\nbody 1=10\n\
         outer\nbody p=1\nbody q=2\ndone\n"
    );
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}

/// Verifies a `break` ends a compiled loop over an interpreted aggregate whose generator never
/// finishes: each value is produced only when the loop asks for it.
#[test]
fn test_compiled_foreach_breaks_out_of_an_endless_interpreted_generator() {
    let out = compile_and_run(
        r#"<?php
eval('class Endless implements IteratorAggregate {
    public function getIterator(): Iterator { $i = 0; while (true) { echo "yield $i\n"; yield $i => $i * 2; $i = $i + 1; } }
}');
function make(string $class): mixed { return new $class(); }
foreach (make('Endless') as $k => $v) { echo "body $k=$v\n"; if ($k === 2) { break; } }
echo "done\n";
"#,
    );
    assert_eq!(out, "yield 0\nbody 0=0\nyield 1\nbody 1=2\nyield 2\nbody 2=4\ndone\n");
}
