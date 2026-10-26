//! Purpose:
//! Exercises native method ownership through subclass, mixed and interface dispatch.
//!
//! Called from:
//! - Runtime GC codegen CI on the supported executable targets.
//!
//! Key details:
//! - Plain builds catch owner markers accidentally masked by heap debugging.
//! - Heap-debug runs also verify borrowed parameter cells and scratch serialization.

use crate::support::*;

/// Native wrapper results and consuming arguments preserve owners through every dispatch route.
#[test]
fn test_intrinsic_method_wrapper_owners_across_dispatch_routes() {
    let source = r#"<?php
class RoutedIntrinsicValue {
    public string $name;
    public function __construct(string $name) { $this->name = $name; }
    public function __destruct() { echo 'drop-', $this->name, '|'; }
}
class RoutedIntrinsicStack extends SplStack {}
class RoutedIntrinsicQueue extends SplQueue {}
class RoutedIntrinsicList extends SplDoublyLinkedList {}
function peekRoutedIntrinsic(mixed $queue): mixed { return $queue->top(); }
function visitRoutedIntrinsic(iterable $iterator): void {
    foreach ($iterator as $key => $value) { echo $key, ':', $value->name, '|'; }
}
function readRoutedIntrinsic(ArrayAccess $source): mixed { return $source[0]; }
$stack = new RoutedIntrinsicStack(); $stack->push(new RoutedIntrinsicValue('subclass'));
$top = $stack->top(); unset($top); $removed = $stack->pop(); unset($removed); echo 'subclass|';
$queue = new SplQueue(); $queue->enqueue(new RoutedIntrinsicValue('mixed'));
$top = peekRoutedIntrinsic($queue); unset($top); $removed = $queue->dequeue(); unset($removed); echo 'mixed|';
$queue = new SplQueue(); $queue->enqueue(new RoutedIntrinsicValue('iterable'));
visitRoutedIntrinsic($queue); unset($queue); echo 'iterable|';
$queue = new RoutedIntrinsicQueue(); $queue->enqueue(new RoutedIntrinsicValue('offset'));
$index = 0; $top = readRoutedIntrinsic($queue); unset($top);
try { $queue->offsetGet(99); } catch (OutOfRangeException $error) { echo 'caught|'; }
$removed = $queue->dequeue(); unset($removed); echo $index, ':offset|';
$list = new RoutedIntrinsicList(); $list->push(17);
$serialized = $list->serialize(); $churn = str_repeat('z', 8000);
echo strlen($serialized), ':', substr($serialized, 0, 4), ':', strlen($churn);
"#;
    let expected = "drop-subclass|subclass|drop-mixed|mixed|0:iterable|drop-iterable|iterable|caught|drop-offset|0:offset|10:i:0;:8000";
    assert_eq!(compile_and_run(source), expected);
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, expected);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// An owning Closure yielded by a compiled iterator releases its capture once per iteration.
#[test]
fn test_compiled_iterator_owned_callable_results_release_captures() {
    let source = r#"<?php
class IteratorCallableCapture { public function __destruct() { echo 'D'; } }
class OwnedCallableIterator implements Iterator {
    private int $index = 0;
    public function current(): Closure {
        $capture = new IteratorCallableCapture();
        return function () use ($capture): int { return 1; };
    }
    public function key(): mixed { return $this->index; }
    public function next(): void { $this->index++; }
    public function rewind(): void { $this->index = 0; }
    public function valid(): bool { return $this->index < 2; }
}
foreach (new OwnedCallableIterator() as $callback) { echo $callback(); }
unset($callback); echo '|end';
"#;
    assert_eq!(compile_and_run(source), "1D1D|end");
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "1D1D|end");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
