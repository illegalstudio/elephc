//! Purpose:
//! End-to-end tests for built-in SPL container classes.
//! Verifies Phase 4 container metadata plus runtime-backed list behavior.
//!
//! Called from:
//! - `cargo test --test codegen_tests` through the SPL test module.
//!
//! Key details:
//! - Runtime tests cover Phase 4 containers; iterator decorators and heaps remain later roadmap phases.

use crate::support::*;

// Tests that Phase 4 SPL classes appear in `spl_classes()`, `get_declared_classes()`,
// and are recognized by `class_exists()` including case-insensitive names.
/// Verifies that phase4 SPL classes are declared for introspection.
#[test]
fn test_phase4_spl_classes_are_declared_for_introspection() {
    let out = compile_and_run(
        r#"<?php
function has_name(array $names, string $target): bool {
    foreach ($names as $name) {
        if ($name === $target) {
            return true;
        }
    }
    return false;
}

$spl = spl_classes();
echo has_name($spl, "SplDoublyLinkedList");
echo has_name($spl, "SplStack");
echo has_name($spl, "SplQueue");
echo has_name($spl, "SplFixedArray");

$declared = get_declared_classes();
echo has_name($declared, "SplDoublyLinkedList");
echo has_name($declared, "SplStack");
echo has_name($declared, "SplQueue");
echo has_name($declared, "SplFixedArray");
echo has_name($declared, "InternalIterator");

var_dump(class_exists("SplDoublyLinkedList"));
var_dump(class_exists("splstack"));
var_dump(class_exists("InternalIterator"));
"#,
    );
    assert_eq!(out, "111111111bool(true)\nbool(true)\nbool(true)\n");
}

// Tests that SplDoublyLinkedList, SplStack, SplQueue, and SplFixedArray implement the
// correct interfaces (Iterator, Countable, ArrayAccess, JsonSerializable) and that
// SplStack/SplQueue inherit from SplDoublyLinkedList.
/// Verifies that phase4 SPL class interface and parent metadata.
#[test]
fn test_phase4_spl_class_interface_and_parent_metadata() {
    let out = compile_and_run(
        r#"<?php
$list = new SplDoublyLinkedList();
var_dump($list instanceof Iterator);
var_dump($list instanceof Countable);
var_dump($list instanceof ArrayAccess);

$stack = new SplStack();
var_dump($stack instanceof SplDoublyLinkedList);
var_dump($stack instanceof Iterator);

$queue = new SplQueue();
var_dump($queue instanceof SplDoublyLinkedList);
var_dump($queue instanceof Countable);

$fixed = new SplFixedArray();
var_dump($fixed instanceof IteratorAggregate);
var_dump($fixed instanceof ArrayAccess);
var_dump($fixed instanceof Countable);
var_dump($fixed instanceof JsonSerializable);
"#,
    );
    assert_eq!(
        out,
        concat!(
            "bool(true)\n",
            "bool(true)\n",
            "bool(true)\n",
            "bool(true)\n",
            "bool(true)\n",
            "bool(true)\n",
            "bool(true)\n",
            "bool(true)\n",
            "bool(true)\n",
            "bool(true)\n",
            "bool(true)\n",
        )
    );
}

// Tests that SplDoublyLinkedList constants (IT_MODE_LIFO, IT_MODE_DELETE, IT_MODE_FIFO)
// are correctly inherited by SplStack and SplQueue with their expected integer values.
/// Verifies that phase4 SPL doubly linked list constants are inherited.
#[test]
fn test_phase4_spl_doubly_linked_list_constants_are_inherited() {
    let out = compile_and_run(
        r#"<?php
echo SplDoublyLinkedList::IT_MODE_LIFO;
echo ",";
echo SplStack::IT_MODE_DELETE;
echo ",";
echo SplQueue::IT_MODE_FIFO;
"#,
    );
    assert_eq!(out, "2,1,0");
}

// Tests SplDoublyLinkedList mutation methods: push, unshift, add, pop, shift, bottom, top,
// count, and isEmpty on a non-empty and empty list.
/// Verifies that phase4 SPL doubly linked list mutation methods.
#[test]
fn test_phase4_spl_doubly_linked_list_mutation_methods() {
    let out = compile_and_run(
        r#"<?php
$list = new SplDoublyLinkedList();
var_dump($list->isEmpty());
$list->push("a");
$list->push(2);
$list->unshift("z");
$list->add(1, "m");
echo count($list);
echo "\n";
echo $list->bottom();
echo "|";
echo $list->top();
echo "\n";
echo $list->shift();
echo "|";
echo $list->pop();
echo "|";
echo $list->shift();
echo "|";
echo $list->pop();
echo "\n";
var_dump($list->isEmpty());
"#,
    );
    assert_eq!(
        out,
        concat!(
            "bool(true)\n",
            "4\n",
            "z|2\n",
            "z|2|m|a\n",
            "bool(true)\n",
        )
    );
}

// Tests SplDoublyLinkedList iteration modes: IT_MODE_FIFO and IT_MODE_LIFO with getIteratorMode/setIteratorMode,
// verifying that foreach produces correct key:value ordering and that the mode value is preserved.
/// Verifies that phase4 SPL doubly linked list iteration modes.
#[test]
fn test_phase4_spl_doubly_linked_list_iteration_modes() {
    let out = compile_and_run(
        r#"<?php
$list = new SplDoublyLinkedList();
$list->push("a");
$list->push("b");
$list->push("c");
foreach ($list as $k => $v) {
    echo $k;
    echo ":";
    echo $v;
    echo ";";
}
echo "\n";
$list->setIteratorMode(SplDoublyLinkedList::IT_MODE_LIFO);
foreach ($list as $k => $v) {
    echo $k;
    echo ":";
    echo $v;
    echo ";";
}
echo "\n";
echo $list->getIteratorMode();
"#,
    );
    assert_eq!(out, "0:a;1:b;2:c;\n2:c;1:b;0:a;\n2");
}

// Tests IT_MODE_DELETE combined with IT_MODE_FIFO and IT_MODE_LIFO: verifies that foreach
// consumes elements during iteration and that count reaches zero after FIFO traversal but
// preserves order for LIFO.
/// Verifies that phase4 SPL doubly linked list delete iteration modes.
#[test]
fn test_phase4_spl_doubly_linked_list_delete_iteration_modes() {
    let out = compile_and_run(
        r#"<?php
$fifo = new SplDoublyLinkedList();
$fifo->push("a");
$fifo->push("b");
$fifo->push("c");
$fifo->setIteratorMode(SplDoublyLinkedList::IT_MODE_FIFO | SplDoublyLinkedList::IT_MODE_DELETE);
foreach ($fifo as $k => $v) {
    echo $k;
    echo ":";
    echo $v;
    echo ";";
}
echo "\n";
echo count($fifo);
echo "\n";

$lifo = new SplDoublyLinkedList();
$lifo->push("a");
$lifo->push("b");
$lifo->push("c");
$lifo->setIteratorMode(SplDoublyLinkedList::IT_MODE_LIFO | SplDoublyLinkedList::IT_MODE_DELETE);
foreach ($lifo as $k => $v) {
    echo $k;
    echo ":";
    echo $v;
    echo ";";
}
echo "\n";
echo count($lifo);
"#,
    );
    assert_eq!(out, "0:a;0:b;0:c;\n0\n2:c;1:b;0:a;\n0");
}

/// Verifies the checked-in SPL delete-iteration mutation stress example observes
/// PHP-compatible traversal order when the active list is mutated inside foreach.
#[test]
fn test_spl_delete_iteration_mutation_example() {
    let out = compile_and_run(include_str!(
        "../../../examples/spl-delete-iteration-mutation/main.php"
    ));
    assert_eq!(out, "0:a|0:b|0:c|0:x|\ncount=0\n");
}

// Tests SplDoublyLinkedList ArrayAccess implementation: offsetExists, offsetGet, offsetSet,
// and offsetUnset via direct bracket access on an empty list, populated list, after unset,
// and that iteration order follows LIFO mode.
/// Verifies that phase4 SPL doubly linked list array access.
#[test]
fn test_phase4_spl_doubly_linked_list_array_access() {
    let out = compile_and_run(
        r#"<?php
$list = new SplDoublyLinkedList();
$list[] = "a";
$list[] = "b";
echo $list[0];
echo "|";
echo $list[1];
echo "\n";
echo isset($list[1]);
echo "\n";
unset($list[0]);
echo $list[0];
echo "\n";
$list[] = "c";
echo $list[1];
echo "\n";
$list[0] = "z";
echo $list[0];
echo "|";
echo count($list);
"#,
    );
    assert_eq!(out, "a|b\n1\nb\nc\nz|2");
}

// Tests RuntimeException from pop/shift/top on empty list and OutOfRangeException from
// offsetSet/add beyond current size, plus that iteration mode affects bracket read order.
/// Verifies that phase4 SPL doubly linked list PHP error edges.
#[test]
fn test_phase4_spl_doubly_linked_list_php_error_edges() {
    let out = compile_and_run(
        r#"<?php
$list = new SplDoublyLinkedList();
try { $list->pop(); } catch (RuntimeException $e) { echo "pop"; }
try { $list->shift(); } catch (RuntimeException $e) { echo "|shift"; }
try { $list->top(); } catch (RuntimeException $e) { echo "|top"; }
$list->push("a");
try { $list[1] = "x"; } catch (OutOfRangeException $e) { echo "|set"; }
try { $list->add(2, "x"); } catch (OutOfRangeException $e) { echo "|add"; }
$list[] = "b";
$list->setIteratorMode(SplDoublyLinkedList::IT_MODE_LIFO);
echo "|";
echo $list[0];
echo $list[1];
"#,
    );
    assert_eq!(out, "pop|shift|top|set|add|ba");
}

// Tests SplStack push/pop/top/count and SplQueue enqueue/dequeue/bottom/top/count
// runtime methods, verifying stack LIFO and queue FIFO ordering.
/// Verifies that phase4 SPL stack and queue runtime methods.
#[test]
fn test_phase4_spl_stack_and_queue_runtime_methods() {
    let out = compile_and_run(
        r#"<?php
$stack = new SplStack();
$stack->push(1);
$stack->push(2);
echo $stack->pop();
echo "|";
echo $stack->top();
echo "|";
echo count($stack);
echo "\n";

$queue = new SplQueue();
$queue->enqueue("a");
$queue->enqueue("b");
echo $queue->dequeue();
echo "|";
echo $queue->bottom();
echo "|";
echo $queue->top();
echo "|";
echo count($queue);
"#,
    );
    assert_eq!(out, "2|1|1\na|b|b|1");
}

// SplStack array offsets are LIFO: offset 0 is the TOP of the stack, so reads, writes and
// unset() address the list in reverse storage order (issue #1659). SplQueue stays FIFO.
/// Verifies that spl stack bracket offsets follow LIFO order.
#[test]
fn test_spl_stack_bracket_offsets_are_lifo() {
    let out = compile_and_run(
        r#"<?php
$s = new SplStack();
$s->push("a"); $s->push("b"); $s->push("c");
echo $s[0], $s[1], $s[2], "\n";
echo $s->offsetGet(0), "\n";
var_dump(isset($s[2]));
$s[0] = "z";
echo $s->top(), $s->bottom(), "\n";
unset($s[0]);
echo count($s), $s->top(), "\n";
$q = new SplQueue();
$q->enqueue("a"); $q->enqueue("b"); $q->enqueue("c");
echo $q[0], $q[1], $q[2], "\n";
$q[0] = "z";
echo $q->bottom(), $q->top(), "\n";
"#,
    );
    assert_eq!(out, "cba\nc\nbool(true)\nza\n2b\nabc\nzc\n");
}

// `getIteratorMode()` reports PHP's raw mode word: `SplStack` = 6 (FIX|LIFO), `SplQueue` = 4
// (FIX), `SplDoublyLinkedList` = 0. `setIteratorMode()` freezes the LIFO/FIFO bit on a fixed
// list (RuntimeException) while allowing the DELETE bit and preserving FIX (issue #1659).
/// Verifies that spl iterator mode word and freeze.
#[test]
fn test_spl_iterator_mode_word_and_freeze() {
    let out = compile_and_run(
        r#"<?php
$s = new SplStack();
$q = new SplQueue();
$d = new SplDoublyLinkedList();
echo $s->getIteratorMode(), $q->getIteratorMode(), $d->getIteratorMode(), "|";
try { $s->setIteratorMode(SplDoublyLinkedList::IT_MODE_FIFO); echo "no-throw"; }
catch (RuntimeException $e) { echo "frozen"; }
$s->setIteratorMode(SplDoublyLinkedList::IT_MODE_LIFO | SplDoublyLinkedList::IT_MODE_DELETE);
echo "|", $s->getIteratorMode();
$s->setIteratorMode(SplDoublyLinkedList::IT_MODE_LIFO);
echo "|", $s->getIteratorMode();
$q->setIteratorMode(SplDoublyLinkedList::IT_MODE_FIFO | SplDoublyLinkedList::IT_MODE_DELETE);
echo "|", $q->getIteratorMode();
$d->setIteratorMode(SplDoublyLinkedList::IT_MODE_LIFO);
echo "|", $d->getIteratorMode();
$d->setIteratorMode(4);
echo "|", $d->getIteratorMode();
$d->setIteratorMode(2);
echo "|", $d->getIteratorMode();
$s->setIteratorMode(PHP_INT_MAX);
echo "|", $s->getIteratorMode();
"#,
    );
    assert_eq!(out, "640|frozen|7|6|5|2|0|2|7");
}

// The LIFO default also drives `foreach` traversal order and `serialize()`'s flags byte; both
// are PHP-parity wins of seeding the mode and were previously unpinned (issue #1659).
/// Verifies that spl stack traversal and serialize follow lifo.
#[test]
fn test_spl_stack_traversal_and_serialize_follow_lifo() {
    let out = compile_and_run(
        r#"<?php
$s = new SplStack();
$s->push("a"); $s->push("b"); $s->push("c");
foreach ($s as $k => $v) { echo $k, ":", $v, " "; }
echo "|", serialize($s), "\n";
"#,
    );
    assert_eq!(
        out,
        "2:c 1:b 0:a |O:8:\"SplStack\":3:{i:0;i:6;i:1;a:3:{i:0;s:1:\"a\";i:1;s:1:\"b\";i:2;s:1:\"c\";}i:2;a:0:{}}\n"
    );
}

// `__debugInfo()` returns the raw mode word and the PHYSICAL storage order (first pushed first),
// not the LIFO `offsetGet()` order, and must snapshot it WITHOUT flipping the frozen mode: the
// old body called `setIteratorMode(0)`, which a fixed-mode list now rejects with a
// RuntimeException (issue #1659).
/// Verifies that spl debug info reports flags and physical order.
#[test]
fn test_spl_debug_info_reports_flags_and_physical_order() {
    let out = compile_and_run(
        r#"<?php
$s = new SplStack();
$s->push("a"); $s->push("b"); $s->push("c");
var_dump($s->__debugInfo());
$q = new SplQueue();
$q->enqueue("a"); $q->enqueue("b"); $q->enqueue("c");
var_dump($q->__debugInfo());
$d = new SplDoublyLinkedList();
$d->push("a"); $d->push("b"); $d->push("c");
var_dump($d->__debugInfo());
"#,
    );
    let entry = |flags: i64| {
        format!(
            "array(2) {{\n  [\"\0SplDoublyLinkedList\0flags\"]=>\n  int({flags})\n  [\"\0SplDoublyLinkedList\0dllist\"]=>\n  array(3) {{\n    [0]=>\n    string(1) \"a\"\n    [1]=>\n    string(1) \"b\"\n    [2]=>\n    string(1) \"c\"\n  }}\n}}\n"
        )
    };
    assert_eq!(out, format!("{}{}{}", entry(6), entry(4), entry(0)));
}

// Tests SplFixedArray getSize/setSize, direct bracket read/write, isset/unset, toArray,
// jsonSerialize, and that resizing a fixed array preserves existing elements up to the new size.
/// Verifies that phase4 SPL fixed array runtime methods.
#[test]
fn test_phase4_spl_fixed_array_runtime_methods() {
    let out = compile_and_run(
        r#"<?php
$fixed = new SplFixedArray(2);
echo count($fixed);
echo "|";
echo $fixed->getSize();
echo "\n";
$fixed[0] = "a";
$fixed[1] = 3;
echo $fixed[0];
echo "|";
echo $fixed[1];
echo "\n";
echo isset($fixed[0]);
unset($fixed[0]);
echo isset($fixed[0]);
echo "\n";
$fixed->setSize(3);
$fixed[2] = "c";
echo count($fixed);
echo "|";
echo $fixed[2];
echo "\n";
$array = $fixed->toArray();
echo count($array);
echo "|";
echo $array[1];
echo "|";
echo $array[2];
echo "\n";
$json = $fixed->jsonSerialize();
echo count($json);
"#,
    );
    // The post-unset `echo isset($fixed[0])` is false → "" in PHP (not "0").
    assert_eq!(out, "2|2\na|3\n1\n3|c\n3|3|c\n3");
}

// Tests that negative size to SplFixedArray constructor throws ValueError, out-of-bounds
// get/set throws OutOfBoundsException, non-integer index throws TypeError, and fromArray
// with string keys throws InvalidArgumentException.
/// Verifies that phase4 SPL fixed array PHP error edges.
#[test]
fn test_phase4_spl_fixed_array_php_error_edges() {
    let out = compile_and_run(
        r#"<?php
try { $tmp = new SplFixedArray(-1); } catch (ValueError $e) { echo "new"; }
$fixed = new SplFixedArray(1);
try { $fixed[1] = "x"; } catch (OutOfBoundsException $e) { echo "|set"; }
try { $x = $fixed[1]; } catch (OutOfBoundsException $e) { echo "|get"; }
try { $fixed["x"] = "x"; } catch (TypeError $e) { echo "|type"; }
try { $fixed->setSize(-1); } catch (ValueError $e) { echo "|resize"; }
try { SplFixedArray::fromArray(["x" => "y"]); } catch (InvalidArgumentException $e) { echo "|from"; }
"#,
    );
    assert_eq!(out, "new|set|get|type|resize|from");
}

// Tests SplDoublyLinkedList serialization helpers: __serialize, __unserialize, __debugInfo,
// serialize, and unserialize, including round-trip preservation of LIFO mode and scalar
// values (true, null).
/// Verifies that phase4 SPL doubly linked list serialization helpers.
#[test]
fn test_phase4_spl_doubly_linked_list_serialization_helpers() {
    let out = compile_and_run(
        r#"<?php
$list = new SplDoublyLinkedList();
$list->push("a");
$list->push(2);
$list->setIteratorMode(SplDoublyLinkedList::IT_MODE_LIFO);

$ser = $list->__serialize();
echo count($ser);
echo "|";
echo $ser[0];
echo "|";
echo count($ser[1]);
echo "|";
echo $ser[1][0];
echo "|";
echo $ser[1][1];
echo "|";
echo count($ser[2]);
echo "\n";

$debug = $list->__debugInfo();
echo count($debug);
echo "\n";

$copy = new SplDoublyLinkedList();
$copy->__unserialize($ser);
echo $copy->getIteratorMode();
echo "|";
echo $copy[0];
echo "|";
echo $copy[1];
echo "\n";

echo $list->serialize();
echo "\n";

$legacy = new SplDoublyLinkedList();
$legacy->unserialize($list->serialize());
echo $legacy->getIteratorMode();
echo "|";
echo $legacy[0];
echo "|";
echo $legacy[1];
echo "\n";

$scalars = new SplDoublyLinkedList();
$scalars->push(true);
$scalars->push(null);
$round = new SplDoublyLinkedList();
$round->unserialize($scalars->serialize());
echo $round[0] ? "true" : "false";
echo "|";
echo is_null($round[1]) ? "null" : "value";
"#,
    );
    assert_eq!(
        out,
        "3|2|2|a|2|0\n2\n2|2|a\ni:2;:s:1:\"a\";:i:2;\n2|2|a\ntrue|null"
    );
}

// Tests SplFixedArray __serialize, __unserialize, and fromArray with both preserve-keys
// (default) and non-preserving (packed) modes, verifying correct null-slot handling and
// size computation.
/// Verifies that phase4 SPL fixed array serialization and from array helpers.
#[test]
fn test_phase4_spl_fixed_array_serialization_and_from_array_helpers() {
    let out = compile_and_run(
        r#"<?php
$fixed = new SplFixedArray(3);
$fixed[1] = "b";
$ser = $fixed->__serialize();
echo count($ser);
echo "|";
echo is_null($ser[0]) ? "null" : $ser[0];
echo "|";
echo $ser[1];
echo "|";
echo is_null($ser[2]) ? "null" : $ser[2];
echo "\n";

$copy = new SplFixedArray();
$copy->__unserialize(["x", "y"]);
echo $copy->getSize();
echo "|";
echo $copy[0];
echo "|";
echo $copy[1];
echo "\n";

$from = SplFixedArray::fromArray([2 => "x", 5 => "y"]);
echo $from->getSize();
echo "|";
echo $from[2];
echo "|";
echo $from[5];
echo "\n";

$packed = SplFixedArray::fromArray([2 => "x", 5 => "y"], false);
echo $packed->getSize();
echo "|";
echo $packed[0];
echo "|";
echo $packed[1];
"#,
    );
    assert_eq!(out, "3|null|b|null\n2|x|y\n6|x|y\n2|x|y");
}

/// Verifies that phase4 SPL fixed array get iterator.
#[test]
fn test_phase4_spl_fixed_array_get_iterator() {
    let out = compile_and_run(
        r#"<?php
$fixed = new SplFixedArray(3);
$fixed[1] = "b";
$it = $fixed->getIterator();
var_dump($it instanceof InternalIterator);
var_dump($it instanceof Iterator);
var_dump($it instanceof SeekableIterator);
foreach ($it as $key => $value) {
    echo $key;
    echo "=";
    echo is_null($value) ? "null" : $value;
    echo ";";
}
echo "\n";
$it->rewind();
$fixed[0] = "a";
echo $it->current();
echo "\n";
foreach ($fixed as $key => $value) {
    echo $key;
    echo "=";
    echo is_null($value) ? "null" : $value;
    echo ";";
}
echo "\n";
$it->next();
$fixed->setSize(1);
var_dump($it->valid());
"#,
    );
    assert_eq!(
        out,
        concat!(
            "bool(true)\n",
            "bool(true)\n",
            "bool(false)\n",
            "0=null;1=b;2=null;\n",
            "a\n",
            "0=a;1=b;2=null;\n",
            "bool(false)\n",
        )
    );
}

// Tests that an SplFixedArray size whose `size * 8` storage payload wraps the machine word is
// rejected by the shared `__rt_array_new` guard instead of allocating a tiny block behind a header
// that advertises 2^61 slots. PHP reports the same class of failure as
// "Possible integer overflow in memory allocation".
/// Verifies that an unrepresentable SplFixedArray size aborts instead of over-reporting capacity.
#[test]
fn test_spl_fixed_array_overflowing_size_is_fatal() {
    let err = compile_and_run_expect_failure(
        r#"<?php
$fixed = new SplFixedArray(0x2000000000000004);
echo $fixed->getSize();
"#,
    );
    assert!(
        err.contains("requested array size exceeds the maximum allowed array size"),
        "{}",
        err
    );
}

// Positive control for the SplFixedArray storage-size guard: an ordinary fixed array still
// allocates, zero-initializes, and round-trips element writes.
/// Verifies that ordinary SplFixedArray allocation is unaffected by the storage-size guard.
#[test]
fn test_spl_fixed_array_normal_size_still_works() {
    let out = compile_and_run(
        r#"<?php
$fixed = new SplFixedArray(3);
$fixed[1] = 7;
echo $fixed->getSize(), ":", $fixed[1], ":", $fixed[0] === null ? "null" : "set";
"#,
    );
    assert_eq!(out, "3:7:null");
}
