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

/// Verifies `count()` and `[]` reach a PHP `Countable`/`ArrayAccess` held in a MIXED slot.
///
/// Both operators go through a runtime helper when the static type is `mixed`, and each helper
/// recognised only a hard-coded list of the runtime's OWN container classes by class id; every
/// other object fell out of the ladder and answered `0` / null. So the same object answered
/// correctly through `$m->count()` and wrongly through `count($m)` — two plausible, silent, wrong
/// values. The synthetic builtins are affected identically, which is why a statically built
/// `ArrayObject` failed this the moment it crossed a `mixed` boundary.
///
/// The parameters are declared `mixed` on purpose: an array literal of objects is typed
/// `array<Real>` by the checker, the operators lower to a direct call, and the bug does not appear.
#[test]
fn test_count_and_offset_get_reach_a_php_interface_through_a_mixed_slot() {
    let out = compile_and_run(
        r#"<?php
class Bag implements Countable, ArrayAccess {
    public array $d = ["k" => 42];
    public function count(): int { return 7; }
    public function offsetExists(mixed $o): bool { return isset($this->d[$o]); }
    public function offsetGet(mixed $o): mixed { return $this->d[$o]; }
    public function offsetSet(mixed $o, mixed $v): void { $this->d[$o] = $v; }
    public function offsetUnset(mixed $o): void { $this->d[$o] = 0; }
}
class InheritedBag extends Bag {}

function counted(mixed $m): int { return count($m); }
function offset(mixed $m): mixed { return $m["k"]; }

echo counted(new Bag()), ":", offset(new Bag()), "\n";
echo counted(new InheritedBag()), ":", offset(new InheritedBag()), "\n";
echo counted(new ArrayObject(["k" => 1, "j" => 2])), ":", offset(new ArrayObject(["k" => 5])), "\n";
"#,
    );
    assert_eq!(out, "7:42\n7:42\n2:5\n");
}

/// Verifies a WRITE through a `mixed` slot reaches `ArrayAccess::offsetSet`.
///
/// The read was fixed first and the write was left behind, which is the worse half: `$m["b"] = 9`
/// on an `ArrayObject` held in a `mixed` slot was DROPPED — the object came back unchanged and
/// nothing was reported. A wrong read at least shows a wrong value; a dropped write shows nothing
/// at all until something downstream reads what was never stored.
///
/// `isset` and `foreach` are checked alongside because they go through different helpers again,
/// and both already answered correctly — pinning them here is what keeps a later change to the
/// write path from being mistaken for a fix to theirs.
#[test]
fn test_offset_set_through_a_mixed_slot_reaches_array_access() {
    let out = compile_and_run(
        r#"<?php
function iss(mixed $m): bool { return isset($m["a"]); }
function wr(mixed $m, mixed $v): void { $m["b"] = $v; }
function it(mixed $m): string { $s = ""; foreach ($m as $k => $v) { $s .= $k . "=" . $v . " "; } return $s; }

$o = new ArrayObject(["a" => 1]);
echo var_export(iss($o), true), "\n";
wr($o, 9);
echo $o->count(), ":", $o->offsetGet("b"), "\n";
wr($o, "texte");
echo $o->offsetGet("b"), "\n";
echo it($o), "\n";
"#,
    );
    assert_eq!(out, "true\n2:9\ntexte\na=1 b=texte \n");
}

/// Verifies writing into an object that is not `ArrayAccess` raises PHP's Error, naming the class.
///
/// Before, the write was silently discarded. Refusing is what PHP does, and it matters more here
/// than on the read: a lost write leaves a program running on state it believes it stored.
#[test]
fn test_offset_set_on_a_non_array_access_object_raises_phps_error() {
    let err = compile_and_run_expect_failure(
        r#"<?php
function wr(mixed $m): void { $m["b"] = 9; }
wr(new stdClass());
echo "unreachable";
"#,
    );
    assert!(
        err.contains("Fatal error: Uncaught Error: Cannot use object of type stdClass as array"),
        "{err}"
    );
}

/// Verifies indexing an object that is not `ArrayAccess` raises PHP's Error, naming the class.
///
/// `$o["k"]` used to answer null for an unrelated class, and to read the PROPERTY for a
/// `stdClass` — a value PHP never produces, since it refuses the syntax outright. The refusal is
/// unconditional: `isset`, `??` and `empty` raise it too, measured against 8.5, which is why this
/// asserts on the quiet context rather than on a plain read. If those contexts were exempt, the
/// helper's warning flag would have had to reach the decision, and it does not.
#[test]
fn test_indexing_a_non_array_access_object_raises_phps_error() {
    let err = compile_and_run_expect_failure(
        r#"<?php
function offset(mixed $m): bool { return isset($m["a"]); }
$o = new stdClass();
$o->a = 1;
var_dump(offset($o));
"#,
    );
    assert!(
        err.contains("Fatal error: Uncaught Error: Cannot use object of type stdClass as array"),
        "{err}"
    );
}

/// Verifies the same refusal names a USER class, not just `stdClass`.
#[test]
fn test_indexing_a_plain_user_object_raises_phps_error() {
    let err = compile_and_run_expect_failure(
        r#"<?php
class Plain { public int $a = 1; }
function offset(mixed $m): mixed { return $m["a"]; }
echo offset(new Plain());
"#,
    );
    assert!(
        err.contains("Fatal error: Uncaught Error: Cannot use object of type Plain as array"),
        "{err}"
    );
}

/// Verifies the runtime's OWN containers still count and index through a `mixed` slot.
///
/// These four are countable without appearing in the dense `Countable::count` table: their `count`
/// is a runtime intrinsic, so no PHP method symbol exists to put there. A countability test built
/// on the table alone would therefore refuse them — and since the refusal is now a fatal, that
/// mistake would turn `count($stack)` from right into a stopped program. The class-id ladder that
/// runs first is what keeps them working, and this pins it.
#[test]
fn test_runtime_containers_still_count_through_a_mixed_slot() {
    let out = compile_and_run(
        r#"<?php
function counted(mixed $m): int { return count($m); }
function offset(mixed $m): mixed { return $m[0]; }

$stack = new SplStack();
$stack->push(1);
$stack->push(2);
$fixed = new SplFixedArray(3);
$fixed[0] = 9;
$queue = new SplQueue();
$queue->enqueue("a");
$list = new SplDoublyLinkedList();
$list->push(5);

echo counted($stack), ":", counted($fixed), ":", offset($fixed), "\n";
echo counted($queue), ":", counted($list), ":", offset($list), "\n";
echo counted([1, 2, 3]), ":", counted(new ArrayIterator(["a" => 1, "b" => 2])), "\n";
"#,
    );
    assert_eq!(out, "2:3:9\n1:1:5\n3:2\n");
}

/// Verifies a `count()` on a STATICALLY known non-`Countable` object refuses at RUN time.
///
/// It used to be a compile error, which looked like free strictness and was not: a program PHP
/// runs to completion did not build. The call here sits in a function nothing calls, so PHP never
/// reaches it and prints `ok` — while elephc refused the whole program over a line that never
/// executes. Both halves are asserted, because fixing only the reachable one would leave the
/// over-refusal in place.
#[test]
fn test_count_on_a_statically_known_non_countable_object_defers_to_run_time() {
    let unreached = compile_and_run(
        r#"<?php
class Plain { public int $a = 1; }
function never_called(): void {
    $p = new Plain();
    echo count($p);
}
echo "ok";
"#,
    );
    assert_eq!(unreached, "ok");

    let reached = compile_and_run_expect_failure(
        r#"<?php
class Plain { public int $a = 1; }
$p = new Plain();
echo count($p);
"#,
    );
    assert!(
        reached.contains(
            "Fatal error: Uncaught TypeError: count(): Argument #1 ($value) \
             must be of type Countable|array, Plain given"
        ),
        "{reached}"
    );
}

/// Verifies counting a non-`Countable` object raises PHP's TypeError, naming the class.
///
/// Two ways to get this wrong, and the class here is built to catch both. Filling the dispatch
/// table from the method NAME alone would make it answer `7`, a value PHP never produces. Leaving
/// the old behaviour in place answers `0`, indistinguishable from an empty container and equally
/// silent, where PHP stops the program.
///
/// The class name has to be resolved at run time — the value arrives as a boxed `Mixed`, so the
/// class is not known while lowering — which is why the message is asserted whole rather than by
/// prefix.
#[test]
fn test_count_through_a_mixed_slot_refuses_a_class_that_does_not_declare_countable() {
    let err = compile_and_run_expect_failure(
        r#"<?php
class Impostor {
    public function count(): int { return 7; }
}
function counted(mixed $m): int { return count($m); }
echo counted(new Impostor());
"#,
    );
    assert!(
        err.contains(
            "Fatal error: Uncaught TypeError: count(): Argument #1 ($value) \
             must be of type Countable|array, Impostor given"
        ),
        "{err}"
    );
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
    assert_eq!(out, "KG|caught\nE|empty\n");
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

/// `empty($obj[$k])` on an `ArrayAccess` receiver asks `offsetExists` first and reads
/// `offsetGet` only when it said yes, with the receiver and the offset evaluated once, as for a
/// call result. Regression for #749, where `empty()` went straight to `offsetGet`.
#[test]
fn test_empty_on_array_access_consults_offset_exists_first() {
    let out = compile_and_run(
        r#"<?php
class C implements ArrayAccess {
    private array $data = ['k' => 1, 'z' => 0, 's' => '', 'a' => [1]];
    public function offsetExists(mixed $o): bool { echo "exists($o)\n"; return $o !== 'missing'; }
    public function offsetGet(mixed $o): mixed { echo "get($o)\n"; return $this->data[$o] ?? null; }
    public function offsetSet(mixed $o, mixed $v): void {}
    public function offsetUnset(mixed $o): void {}
}
function make(): C { echo "make\n"; return new C(); }
function key_of(string $k): string { echo "key\n"; return $k; }
$c = new C();
var_dump(empty($c['k']));
var_dump(empty($c['z']));
var_dump(empty($c['s']));
var_dump(empty($c['a']));
var_dump(empty($c['missing']));
var_dump(isset($c['j']));
var_dump(empty(make()[key_of('k')]));
var_dump(!empty($c['k']));
if (empty($c['missing'])) { echo "branch empty\n"; }
$arr = ['x' => 0];
var_dump(empty($arr['x']), empty($arr['nope']));
"#,
    );
    assert_eq!(
        out,
        concat!(
            "exists(k)\n",
            "get(k)\n",
            "bool(false)\n",
            "exists(z)\n",
            "get(z)\n",
            "bool(true)\n",
            "exists(s)\n",
            "get(s)\n",
            "bool(true)\n",
            "exists(a)\n",
            "get(a)\n",
            "bool(false)\n",
            "exists(missing)\n",
            "bool(true)\n",
            "exists(j)\n",
            "bool(true)\n",
            "make\n",
            "key\n",
            "exists(k)\n",
            "get(k)\n",
            "bool(false)\n",
            "exists(k)\n",
            "get(k)\n",
            "bool(true)\n",
            "exists(missing)\n",
            "branch empty\n",
            "bool(true)\n",
            "bool(true)\n",
        )
    );
}

/// The value `empty()` reads through `offsetGet` is released once tested, for a variable
/// receiver and for a call result.
#[test]
fn test_empty_on_array_access_leaves_a_clean_heap() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class Bag implements ArrayAccess {
    public function __construct(private array $data) {}
    public function offsetExists(mixed $o): bool { return isset($this->data[$o]); }
    public function offsetGet(mixed $o): mixed { return $this->data[$o]; }
    public function offsetSet(mixed $o, mixed $v): void {}
    public function offsetUnset(mixed $o): void {}
}
function bag(): Bag { return new Bag(['a' => str_repeat('x', 3), 'z' => '']); }
function rows(): array { return ['r' => 'v']; }
$b = bag();
$n = 0;
for ($i = 0; $i < 40; $i++) {
    $k = $i % 2 ? 'a' : 'z' . '';
    if (empty($b[$k])) { $n++; }
    if (empty(bag()[$k . ''])) { $n++; }
    if (!empty(rows()['r'])) { $n++; }
    if (empty(rows()['missing'])) { $n++; }
}
echo $n, "\n";
"#,
    );
    assert_eq!(out.stdout, "120\n", "stderr: {}", out.stderr);
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean heap, got: {}",
        out.stderr
    );
}
