<?php
// SPL data structures: SplStack, SplQueue, SplDoublyLinkedList, SplFixedArray.
// Each one is a regular PHP class — push, pop, count, isEmpty, iterate.

echo "-- SplStack (LIFO) --\n";
$stack = new SplStack();
$stack->push("first");
$stack->push("second");
$stack->push("third");
echo "count: " . $stack->count() . "\n";
echo "top:   " . $stack->top() . "\n";
echo "pop:   " . $stack->pop() . "\n";
echo "pop:   " . $stack->pop() . "\n";
echo "empty: " . ($stack->isEmpty() ? "yes" : "no") . "\n";

echo "\n-- SplQueue (FIFO) --\n";
$queue = new SplQueue();
$queue->enqueue("alice");
$queue->enqueue("bob");
$queue->enqueue("carol");
echo "count: " . $queue->count() . "\n";
echo "deq:   " . $queue->dequeue() . "\n";
echo "deq:   " . $queue->dequeue() . "\n";

echo "\n-- SplDoublyLinkedList (push/pop/shift/unshift) --\n";
$list = new SplDoublyLinkedList();
$list->push("middle");
$list->unshift("head");
$list->push("tail");
echo "bottom: " . $list->bottom() . "\n";
echo "top:    " . $list->top() . "\n";
echo "shift:  " . $list->shift() . "\n";
echo "shift:  " . $list->shift() . "\n";
echo "shift:  " . $list->shift() . "\n";

echo "\n-- SplFixedArray (bounded) --\n";
$fa = new SplFixedArray(4);
$fa->offsetSet(0, "alpha");
$fa->offsetSet(1, "beta");
$fa->offsetSet(2, "gamma");
$fa->offsetSet(3, "delta");
echo "size: " . $fa->getSize() . "\n";
echo "fa[2]: " . $fa->offsetGet(2) . "\n";

try {
    $fa->offsetGet(99);
} catch (RuntimeException $e) {
    echo "caught: " . $e->getMessage() . "\n";
}
