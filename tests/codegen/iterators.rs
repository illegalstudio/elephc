use crate::support::*;

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
