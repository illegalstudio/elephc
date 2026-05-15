//! Purpose:
//! Foreach interop with user-defined Iterator and IteratorAggregate classes — confirms generator-style iteration cooperates with the broader iterator protocol.
//!
//! Called from:
//!  - `cargo test` via the integration test harness; aggregated under
//!    `tests::codegen::generators` in `tests/codegen/generators/mod.rs`.
//!
//! Key details:
//!  - Ensures Generator support remains compatible with existing Iterator and
//!    IteratorAggregate dispatch paths.

use crate::support::*;

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
