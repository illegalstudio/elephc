//! Purpose:
//! Regression tests for method calls on nullable receivers of built-in classes, `count()` over
//! nullable objects, and the SPL list classes' ownership and traversal direction.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - A `?C` receiver stayed boxed in the call's non-null branch, so the backend took its nullable
//!   path, which knows ordinary method bodies but none of the built-in intrinsics: a built-in
//!   exception method jumped to an empty vtable slot, `?SplStack->top()` answered the stack, and
//!   `?Generator->current()` dispatched over every `Iterator` and demanded pcre2.
//! - The SPL list and fixed-array writers adopt their value box, while the caller released the box
//!   it passed: the next allocation of that block rewrote the stored element.
//! - `SplStack` and `SplQueue` start in php's frozen LIFO and FIFO modes, and `unshift()` prepends
//!   to the list whatever its direction.
//! - `array_splice()` over refcounted elements moves the removed ones into its result.

use crate::support::*;

/// Method calls on `?LogicException`, a user exception subclass, `?Generator`, `?ArrayObject`,
/// `?SplStack`, `?ArrayIterator`, `?Closure` and `?Fiber` receivers answer like their typed forms,
/// and a null receiver raises php's `Error`.
#[test]
fn test_nullable_builtin_class_receivers_dispatch_like_typed_ones() {
    let out = compile_and_run(
        r#"<?php
class X extends LogicException {}
function le(): ?LogicException { return new LogicException("q", 5, new RuntimeException("p")); }
function xe(): ?X { return new X("x"); }
function ue(): ?DomainException { return null; }
function gen(): ?Generator { return (function () { yield 1; yield 2; })(); }
function ao(): ?ArrayObject { return new ArrayObject([1, 2, 3]); }
function st(): ?SplStack { $s = new SplStack(); $s->push(4); $s->push(5); return $s; }
function ai(): ?ArrayIterator { return new ArrayIterator(["a" => 1]); }
function cl(): ?Closure { return function ($x) { return $x * 2; }; }
function fb(): ?Fiber { return new Fiber(function () { Fiber::suspend(3); }); }
function msg(?Throwable $t): string { return $t->getMessage(); }
echo le()->getMessage(), le()->getCode(), le()->getPrevious()->getMessage(), le()->getLine(), "\n";
$x = xe();
echo $x->getMessage(), get_class($x), msg($x), "\n";
try { echo ue()->getMessage(); } catch (Error $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
$g = gen();
echo $g->current(), gen()->current(), "\n";
echo ao()->count(), st()->top(), st()->pop(), count(st()), ai()->key(), "\n";
echo cl()->__invoke(4), "\n";
$f = fb();
echo $f->start(), fb()->isStarted() ? "y" : "n", "\n";
"#,
    );
    assert_eq!(out, "q5p3\nxXx\nError: Call to a member function getMessage() on null\n11\n3552a\n8\n3n\n");
}

/// `SplStack` iterates, indexes and inserts from the top, `unshift()` prepends at the bottom of
/// any list, and the frozen direction of a stack or queue refuses to turn around.
#[test]
fn test_spl_stack_and_queue_traverse_in_their_frozen_direction() {
    let out = compile_and_run(
        r#"<?php
function show($l) { $o = []; foreach ($l as $v) { $o[] = $v; } echo json_encode($o), "\n"; }
$s = new SplStack(); $s->push(1); $s->push(2); $s->push(3);
show($s);
$o = []; foreach ($s as $v) { $o[] = $v; } echo json_encode($o), "\n";
echo $s->getIteratorMode(), "\n";
echo json_encode(iterator_to_array($s, false)), "\n";
$s->add(0, 9);
$o = []; foreach ($s as $v) { $o[] = $v; } echo json_encode($o), "\n";
echo $s->top(), $s[0], $s[1], "\n";
$s = new SplStack(); $s->push(1); $s->push(2); $s->push(3); $s->add(0, 9); show($s);
$s = new SplStack(); $s->push(1); $s->push(2); $s->push(3); $s->add(1, 9); show($s);
$s = new SplStack(); $s->push(1); $s->push(2); $s->push(3); $s->add(3, 9); show($s);
$s = new SplStack(); $s->push(1); $s->push(2); $s->unshift(0); show($s);
$s = new SplStack(); $s->push(1); $s->push(2); $s->push(3); $s[0] = 8; $s[2] = 7; show($s);
$d = new SplDoublyLinkedList(); $d->push(1); $d->push(2); $d->push(3); $d->add(1, 9); show($d);
$q = new SplQueue(); $q->push(1); $q->push(2); $q->add(0, 9); show($q);
$s = new SplStack();
echo $s->getIteratorMode(), " ", (new SplQueue())->getIteratorMode(), " ", (new SplDoublyLinkedList())->getIteratorMode(), "\n";
$s->setIteratorMode(SplDoublyLinkedList::IT_MODE_LIFO | SplDoublyLinkedList::IT_MODE_DELETE);
echo $s->getIteratorMode(), "\n";
try { $s->setIteratorMode(0); } catch (RuntimeException $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
echo $s->getIteratorMode(), "\n";
$d = new SplDoublyLinkedList();
$d->setIteratorMode(6);
echo $d->getIteratorMode(), "\n";
$q = new SplQueue();
$q->push(1); $q->push(2); $q->unshift(0);
foreach ($q as $v) { echo $v; }
echo "\n";
$s = new SplStack();
$s->push(1); $s->push(2);
$s->setIteratorMode(SplDoublyLinkedList::IT_MODE_LIFO | SplDoublyLinkedList::IT_MODE_DELETE);
foreach ($s as $v) { echo $v; }
echo " left:", count($s), "\n";
echo serialize(new SplStack()), "\n";
"#,
    );
    assert_eq!(out, "[3,2,1]\n[3,2,1]\n6\n[3,2,1]\n[3,9,2,1]\n339\n[3,9,2,1]\n[3,2,9,1]\n[9,3,2,1]\n[2,1,0]\n[8,2,7]\n[1,9,2,3]\n[9,1,2]\n6 4 0\n7\nRuntimeException: Iterators' LIFO/FIFO modes for SplStack/SplQueue objects are frozen\n7\n2\n012\n21 left:0\nO:8:\"SplStack\":3:{i:0;i:6;i:1;a:0:{}i:2;a:0:{}}\n");
}

/// `count()` on `?SplStack` or `?Countable` compiles and raises php's `TypeError` for null or a
/// non-countable object; `SplObjectStorage::offsetSet()`'s `$info` is optional.
#[test]
fn test_count_accepts_nullable_countable_objects() {
    let out = compile_and_run(
        r#"<?php
class C implements Countable { public function count(): int { return 3; } }
class P {}
function st(): ?SplStack { $s = new SplStack(); $s->push(4); $s->push(5); return $s; }
function cc(): ?C { return new C(); }
function nn(): ?C { return null; }
function pp(): ?P { return new P(); }
echo count(st()), count(cc()), "\n";
$x = st();
echo count($x), "\n";
try { echo count(nn()); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
try { echo count(pp()); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
$s = new SplObjectStorage();
$o = new stdClass();
$s->offsetSet($o);
echo count($s), "\n";
"#,
    );
    assert_eq!(out, "23\n2\ncount(): Argument #1 ($value) must be of type Countable|array, null given\ncount(): Argument #1 ($value) must be of type Countable|array, P given\n1\n");
}

/// `push`, `unshift`, `add`, `enqueue`, `[]=` and `offsetSet` keep the values they are given, and
/// release them exactly once with the list.
#[test]
fn test_spl_list_writers_keep_their_values_and_release_them_once() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function run(int $round): string {
    $out = "";
    $s = new SplStack();
    $m = $round % 2 ? "odd" . $round : [$round, "x"];
    $s->push(7);
    $s->push("str" . $round);
    $s->push($m);
    $s[] = $m;
    $s[] = 9;
    $s->unshift($m);
    $s->add(1, $m);
    $s->add(0, 5);
    $s[0] = $m;
    $s[1] = "w" . $round;
    $junk = [];
    for ($i = 0; $i < 4; $i++) { $junk[] = [$i, "z" . $i]; }
    $out .= json_encode(iterator_to_array($s, false));
    $q = new SplQueue();
    $q->enqueue($m);
    $q->enqueue(3);
    $q[] = $m;
    $out .= json_encode([$q->dequeue(), $q->dequeue(), $q->dequeue()]);
    $f = new SplFixedArray(3);
    $f[0] = $m;
    $f[1] = 4;
    $f[2] = "q" . $round;
    $f[0] = "replaced";
    $out .= json_encode($f->toArray());
    return $out;
}
for ($r = 0; $r < 4; $r++) { echo run($r), "\n"; }
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "[[0,\"x\"],\"w0\",[0,\"x\"],[0,\"x\"],[0,\"x\"],\"str0\",7,[0,\"x\"]][[0,\"x\"],3,[0,\"x\"]][\"replaced\",4,\"q0\"]\n[\"odd1\",\"w1\",\"odd1\",\"odd1\",\"odd1\",\"str1\",7,\"odd1\"][\"odd1\",3,\"odd1\"][\"replaced\",4,\"q1\"]\n[[2,\"x\"],\"w2\",[2,\"x\"],[2,\"x\"],[2,\"x\"],\"str2\",7,[2,\"x\"]][[2,\"x\"],3,[2,\"x\"]][\"replaced\",4,\"q2\"]\n[\"odd3\",\"w3\",\"odd3\",\"odd3\",\"odd3\",\"str3\",7,\"odd3\"][\"odd3\",3,\"odd3\"][\"replaced\",4,\"q3\"]\n");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap, got: {}",
        out.stderr
    );
}

/// `array_splice()` over refcounted elements MOVES the removed ones into its result: each kept a
/// second reference before, so spliced-out boxes, arrays and objects leaked and their destructors
/// never ran.
#[test]
fn test_array_splice_moves_removed_refcounted_elements() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class O { public function __construct(public int $v) {} public function __destruct() { echo "~", $this->v; } }
function run(array $a): array {
    $r = array_splice($a, 1, 2);
    echo count($a), ":", json_encode($r), "\n";
    return $a;
}
$x = run([1, "two", [3], 4.5]);
echo json_encode($x), "\n";
$objs = [new O(1), new O(2), new O(3)];
$gone = array_splice($objs, 0, 2);
echo " kept:", count($objs), " gone:", count($gone), "\n";
unset($gone);
echo "\nafter\n";
$m = [[1], [2], [3]];
$copy = $m;
array_splice($m, 1, 1, ["r"]);
echo json_encode($m), json_encode($copy), "\n";
$h = [1, "a", 2.5];
array_splice($h, -1);
echo json_encode($h), "\n";
unset($objs);
echo "\nend\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "2:[\"two\",[3]]\n[1,4.5]\n kept:1 gone:2\n~1~2\nafter\n[[1],\"r\",[3]][[1],[2],[3]]\n[1,\"a\"]\n~3\nend\n");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap, got: {}",
        out.stderr
    );
}
