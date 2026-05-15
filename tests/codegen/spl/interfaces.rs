//! Purpose:
//! End-to-end tests for SPL builtin interfaces and their PHP-compatible contracts.
//!
//! Called from:
//! - `cargo test --test codegen_tests` through Rust's test harness.
//!
//! Key details:
//! - These fixtures exercise checker validation plus runtime `instanceof` metadata.

use crate::support::*;

#[test]
fn test_countable_interface_implementer_typechecks_and_runs() {
    let out = compile_and_run(
        r#"<?php
class Counter implements Countable {
    public function __construct(private int $n) {}
    public function count(): int { return $this->n; }
}
$c = new Counter(7);
echo $c->count();
"#,
    );
    assert_eq!(out, "7");
}

#[test]
fn test_countable_instanceof_succeeds() {
    let out = compile_and_run(
        r#"<?php
class Counter implements Countable {
    public function count(): int { return 0; }
}
$c = new Counter();
var_dump($c instanceof Countable);
"#,
    );
    assert_eq!(out, "bool(true)\n");
}

#[test]
fn test_builtin_interface_names_are_case_insensitive() {
    let out = compile_and_run(
        r#"<?php
class Counter implements \countable {
    public function count(): int { return 3; }
}
$c = new Counter();
echo count($c);
var_dump($c instanceof Countable);
"#,
    );
    assert_eq!(out, "3bool(true)\n");
}

#[test]
fn test_traversable_inherited_via_iterator() {
    let out = compile_and_run(
        r#"<?php
class Range implements Iterator {
    private int $i = 0;
    public function __construct(private int $n) {}
    public function current(): mixed { return $this->i; }
    public function key(): mixed { return $this->i; }
    public function next(): void { $this->i = $this->i + 1; }
    public function valid(): bool { return $this->i < $this->n; }
    public function rewind(): void { $this->i = 0; }
}
$r = new Range(3);
var_dump($r instanceof Iterator);
var_dump($r instanceof Traversable);
"#,
    );
    assert_eq!(out, "bool(true)\nbool(true)\n");
}

#[test]
fn test_iterator_aggregate_get_iterator_accepts_traversable_return() {
    let out = compile_and_run(
        r#"<?php
class RangeIter implements Iterator {
    public function current(): mixed { return 1; }
    public function key(): mixed { return 0; }
    public function next(): void {}
    public function valid(): bool { return false; }
    public function rewind(): void {}
}
class Bag implements IteratorAggregate {
    public function getIterator(): Traversable { return new RangeIter(); }
}
$b = new Bag();
var_dump($b instanceof IteratorAggregate);
"#,
    );
    assert_eq!(out, "bool(true)\n");
}

#[test]
fn test_outer_iterator_inherits_iterator_methods() {
    let out = compile_and_run(
        r#"<?php
class Wrap implements OuterIterator {
    public function __construct(private Iterator $inner) {}
    public function getInnerIterator(): ?Iterator { return $this->inner; }
    public function current(): mixed { return $this->inner->current(); }
    public function key(): mixed { return $this->inner->key(); }
    public function next(): void { $this->inner->next(); }
    public function valid(): bool { return $this->inner->valid(); }
    public function rewind(): void { $this->inner->rewind(); }
}
class Range implements Iterator {
    private int $i = 0;
    public function __construct(private int $n) {}
    public function current(): mixed { return $this->i; }
    public function key(): mixed { return $this->i; }
    public function next(): void { $this->i = $this->i + 1; }
    public function valid(): bool { return $this->i < $this->n; }
    public function rewind(): void { $this->i = 0; }
}
$w = new Wrap(new Range(2));
var_dump($w instanceof OuterIterator);
var_dump($w instanceof Iterator);
"#,
    );
    assert_eq!(out, "bool(true)\nbool(true)\n");
}

#[test]
fn test_seekable_iterator_extends_iterator() {
    let out = compile_and_run(
        r#"<?php
class Track implements SeekableIterator {
    private int $pos = 0;
    public function seek(int $offset): void { $this->pos = $offset; }
    public function current(): mixed { return $this->pos; }
    public function key(): mixed { return $this->pos; }
    public function next(): void { $this->pos = $this->pos + 1; }
    public function valid(): bool { return $this->pos < 10; }
    public function rewind(): void { $this->pos = 0; }
}
$t = new Track();
$t->seek(4);
echo $t->current();
"#,
    );
    assert_eq!(out, "4");
}

#[test]
fn test_recursive_iterator_extends_iterator() {
    let out = compile_and_run(
        r#"<?php
class Node implements RecursiveIterator {
    public function __construct(private int $depth) {}
    public function getChildren(): ?RecursiveIterator { return null; }
    public function hasChildren(): bool { return false; }
    public function current(): mixed { return $this->depth; }
    public function key(): mixed { return $this->depth; }
    public function next(): void {}
    public function valid(): bool { return false; }
    public function rewind(): void {}
}
$n = new Node(3);
echo $n->current();
var_dump($n instanceof Iterator);
"#,
    );
    assert_eq!(out, "3bool(true)\n");
}

#[test]
fn test_spl_observer_subject_interfaces() {
    // Property access through interface-typed parameters isn't supported,
    // so this fixture only exercises the interface contract itself.
    let out = compile_and_run(
        r#"<?php
class Subject implements SplSubject {
    public function attach(SplObserver $observer): void {}
    public function detach(SplObserver $observer): void {}
    public function notify(): void {}
}
class Watcher implements SplObserver {
    public int $seen = 0;
    public function update(SplSubject $subject): void { $this->seen = 1; }
}
$s = new Subject();
$w = new Watcher();
$w->update($s);
echo $w->seen;
var_dump($w instanceof SplObserver);
var_dump($s instanceof SplSubject);
"#,
    );
    assert_eq!(out, "1bool(true)\nbool(true)\n");
}

#[test]
fn test_stringable_interface_runs() {
    let out = compile_and_run(
        r#"<?php
class Stamp implements Stringable {
    public function __construct(private string $label) {}
    public function __toString(): string { return "[" . $this->label . "]"; }
}
$s = new Stamp("hi");
echo (string)$s;
var_dump($s instanceof Stringable);
"#,
    );
    assert_eq!(out, "[hi]bool(true)\n");
}

#[test]
fn test_tostring_method_implicitly_implements_stringable() {
    let out = compile_and_run(
        r#"<?php
class Stamp {
    public function __construct(private string $label) {}
    public function __toString(): string { return "[" . $this->label . "]"; }
}
$s = new Stamp("hi");
echo (string)$s;
var_dump($s instanceof Stringable);
"#,
    );
    assert_eq!(out, "[hi]bool(true)\n");
}

#[test]
fn test_json_serializable_interface_typechecks() {
    let out = compile_and_run(
        r#"<?php
class Boxed implements JsonSerializable {
    public function __construct(private int $n) {}
    public function jsonSerialize(): mixed { return $this->n; }
}
$b = new Boxed(42);
var_dump($b instanceof JsonSerializable);
echo $b->jsonSerialize();
"#,
    );
    assert_eq!(out, "bool(true)\n42");
}

#[test]
fn test_array_access_interface_typechecks() {
    // ArrayAccess subscript syntax ($obj[$k]) is deferred to a follow-up
    // PR; this test verifies the interface contract is enforced and methods
    // are callable directly.
    let out = compile_and_run(
        r#"<?php
class Box implements ArrayAccess {
    private string $stored = "";
    public function offsetExists(mixed $offset): bool { return $this->stored !== ""; }
    public function offsetGet(mixed $offset): mixed { return $this->stored; }
    public function offsetSet(mixed $offset, mixed $value): void { $this->stored = (string)$value; }
    public function offsetUnset(mixed $offset): void { $this->stored = ""; }
}
$b = new Box();
$b->offsetSet("k", "v");
echo $b->offsetGet("k");
var_dump($b instanceof ArrayAccess);
"#,
    );
    assert_eq!(out, "vbool(true)\n");
}
