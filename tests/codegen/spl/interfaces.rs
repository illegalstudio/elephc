//! Purpose:
//! End-to-end tests for SPL builtin interfaces and their PHP-compatible contracts.
//!
//! Called from:
//! - `cargo test --test codegen_tests` through Rust's test harness.
//!
//! Key details:
//! - These fixtures exercise checker validation plus runtime `instanceof` metadata.

use crate::support::*;

/// Verifies a class implementing `Countable` typechecks and that `count()` is callable.
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

/// Verifies `instanceof` returns `true` for a `Countable` implementer.
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

/// Verifies SPL builtin interface names are case-insensitive (e.g., `\countable` and `Countable`).
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

/// Verifies a class implementing `Iterator` automatically satisfies `Traversable`
/// (since `Iterator` extends `Traversable`).
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

/// Verifies `IteratorAggregate::getIterator()` can return a `Traversable`
/// ( covariant return type: `Iterator` → `Traversable`).
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

/// Verifies `OuterIterator` implementers also satisfy `Iterator` via inheritance.
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

/// Verifies `SeekableIterator` extenders satisfy `Iterator` and that `seek()` works.
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

/// Verifies `RecursiveIterator` extenders satisfy `Iterator` and the additional
/// `getChildren()`/`hasChildren()` methods are callable.
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

/// Verifies `SplSubject`/`SplObserver` attach/detach/notify/update contract,
/// including `instanceof` for both interfaces.
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

/// Verifies `Stringable` implementer typechecks and `__toString()` is invoked on cast.
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

/// Verifies a class with `__toString()` implicitly satisfies `Stringable`
/// (no explicit `implements Stringable` needed).
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

/// Verifies `JsonSerializable` implementer typechecks and `jsonSerialize()` is callable.
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

/// Verifies `ArrayAccess` implementer typechecks with offsetExists/Get/Set/Unset methods.
#[test]
fn test_array_access_interface_typechecks() {
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

/// Verifies subscript operations `[]=` / `[]` / `isset()` / `unset()` dispatch via
/// `ArrayAccess` interface (with trace letters to confirm each method is called).
#[test]
fn test_array_access_subscript_read_write_isset_unset() {
    let out = compile_and_run(
        r#"<?php
class Box implements ArrayAccess {
    private string $stored = "";
    public function offsetExists(mixed $offset): bool { echo "E"; return $this->stored !== ""; }
    public function offsetGet(mixed $offset): mixed { echo "G"; return $this->stored; }
    public function offsetSet(mixed $offset, mixed $value): void { echo "S"; $this->stored = (string)$value; }
    public function offsetUnset(mixed $offset): void { echo "U"; $this->stored = ""; }
}
$b = new Box();
$b["k"] = "v";
echo $b["k"];
echo isset($b["k"]);
unset($b["k"]);
echo isset($b["k"]);
"#,
    );
    // Final `isset` is false (offsetExists returns false after unset); a bool
    // false echoes as "" in PHP (not "0"), so the trace ends "…UE", not "…UE0".
    assert_eq!(out, "SGvE1UE");
}

/// Verifies the checked-in ArrayAccess exception-order stress example preserves key
/// evaluation side effects before an offsetGet exception unwinds to the catch block.
#[test]
fn test_array_access_exception_side_effect_order_example() {
    let out = compile_and_run(include_str!(
        "../../../examples/array-access-exception-order/main.php"
    ));
    assert_eq!(out, "KG|caught\n");
}

/// Verifies subscript operations work when an `ArrayAccess` implementer is passed
/// through an interface-typed parameter (dispatch via interface type, not concrete type).
#[test]
fn test_array_access_subscript_dispatches_through_interface_type() {
    let out = compile_and_run(
        r#"<?php
class Box implements ArrayAccess {
    private string $stored = "";
    public function offsetExists(mixed $offset): bool { return $this->stored !== ""; }
    public function offsetGet(mixed $offset): mixed { return $this->stored; }
    public function offsetSet(mixed $offset, mixed $value): void { $this->stored = (string)$value; }
    public function offsetUnset(mixed $offset): void { $this->stored = ""; }
}
function use_box_slot(ArrayAccess $box): void {
    $box["k"] = "v";
    echo $box["k"];
    echo isset($box["k"]);
    unset($box["k"]);
    echo isset($box["k"]);
}
use_box_slot(new Box());
"#,
    );
    // Final `isset` is false → bool false echoes as "" in PHP (not "0"): "v1".
    assert_eq!(out, "v1");
}

/// Verifies subscript operations work on `ArrayAccess` through property (`$obj->prop[key]`)
/// and static property (`Class::$prop[key]`) syntax.
#[test]
fn test_array_access_subscript_property_and_static_property_writes() {
    let out = compile_and_run(
        r#"<?php
class Box implements ArrayAccess {
    private string $stored = "";
    public function offsetExists(mixed $offset): bool { return $this->stored !== ""; }
    public function offsetGet(mixed $offset): mixed { return $this->stored; }
    public function offsetSet(mixed $offset, mixed $value): void { $this->stored = (string)$value; }
    public function offsetUnset(mixed $offset): void { $this->stored = ""; }
}
class Holder {
    public Box $box;
    public static Box $staticBox;
    public function __construct() {
        $this->box = new Box();
    }
}
$holder = new Holder();
$holder->box["k"] = "p";
echo $holder->box["k"];
Holder::$staticBox = new Box();
Holder::$staticBox["k"] = "s";
echo Holder::$staticBox["k"];
"#,
    );
    assert_eq!(out, "ps");
}

/// Verifies subscript assignment expressions (`$b["k"] = 5`), compound assignment
/// (`+=`), and null-coalescing assignment (`??=`) return the computed value.
#[test]
fn test_array_access_assignment_expression_returns_computed_value() {
    let out = compile_and_run(
        r#"<?php
class AssignBox implements ArrayAccess {
    private int $stored = 0;
    public function offsetExists(mixed $offset): bool { return true; }
    public function offsetGet(mixed $offset): mixed { echo "G"; return $this->stored; }
    public function offsetSet(mixed $offset, mixed $value): void { echo "S"; $this->stored = 99; }
    public function offsetUnset(mixed $offset): void {}
}
$b = new AssignBox();
echo "=";
echo ($b["k"] = 5);
echo "|";
class CounterBox implements ArrayAccess {
    private int $stored = 15;
    public function offsetExists(mixed $offset): bool { return true; }
    public function offsetGet(mixed $offset): int { echo "G"; return $this->stored; }
    public function offsetSet(mixed $offset, mixed $value): void { echo "S"; $this->stored = 99; }
    public function offsetUnset(mixed $offset): void {}
}
$counter = new CounterBox();
echo ($counter["k"] += 2);
echo "|";
class MaybeBox implements ArrayAccess {
    private int $stored = 0;
    public function offsetExists(mixed $offset): bool { return false; }
    public function offsetGet(mixed $offset): mixed { echo "G"; return null; }
    public function offsetSet(mixed $offset, mixed $value): void { echo "S"; $this->stored = 1; }
    public function offsetUnset(mixed $offset): void {}
}
$c = new MaybeBox();
echo ($c["k"] ??= 3);
"#,
    );
    assert_eq!(out, "=S5|GS17|GS3");
}

/// Verifies subscript read on a union type (`LeftBox|RightBox`) dispatches to the
/// correct implementer based on the actual runtime type.
#[test]
fn test_array_access_union_uses_interface_dispatch() {
    let out = compile_and_run(
        r#"<?php
class LeftBox implements ArrayAccess {
    public function offsetExists(mixed $offset): bool { return (string)$offset === "k"; }
    public function offsetGet(mixed $offset): mixed { return "L"; }
    public function offsetSet(mixed $offset, mixed $value): void {}
    public function offsetUnset(mixed $offset): void {}
}
class RightBox implements ArrayAccess {
    public function beforeOne(): string { return "x"; }
    public function beforeTwo(): string { return "y"; }
    public function offsetExists(mixed $offset): bool { return (string)$offset === "k"; }
    public function offsetGet(mixed $offset): mixed { return "R"; }
    public function offsetSet(mixed $offset, mixed $value): void {}
    public function offsetUnset(mixed $offset): void {}
}
function choose_box(bool $left): LeftBox|RightBox {
    if ($left) {
        return new LeftBox();
    }
    return new RightBox();
}
echo choose_box(true)["k"];
echo choose_box(false)["k"];
"#,
    );
    assert_eq!(out, "LR");
}
