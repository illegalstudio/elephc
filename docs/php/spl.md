---
title: "SPL"
description: "Standard PHP Library — exception hierarchy, iterator interfaces, and the core data-structure classes (SplStack, SplQueue, SplDoublyLinkedList, SplFixedArray)."
sidebar:
  order: 11
---

elephc ships a subset of the Standard PHP Library that AOT compilation can reasonably support: the SPL exception hierarchy, the `Iterator` / `Countable` / `ArrayAccess` interfaces, and a first batch of data-structure classes. Everything in this page is available out of the box — no `use` import, no autoload registration. The class names live in the global namespace, just like in stock PHP.

For SPL autoload functions (`spl_autoload_register`, `spl_autoload_extensions`, …) and class-introspection helpers (`spl_classes`, `spl_object_id`, …) see [Namespaces](namespaces.md).

## Exception hierarchy

elephc inlines the standard SPL exception classes so user code can `throw` and `catch` them like any built-in:

```
Exception
├── LogicException
│   ├── BadFunctionCallException
│   │   └── BadMethodCallException
│   ├── DomainException
│   ├── InvalidArgumentException
│   ├── LengthException
│   └── OutOfRangeException
└── RuntimeException
    ├── OutOfBoundsException
    ├── OverflowException
    ├── RangeException
    ├── UnderflowException
    └── UnexpectedValueException
```

Catch order follows PHP semantics — the first matching `catch` block, walking the hierarchy upward, wins. `catch (Exception $e)` matches anything in the tree.

## Interfaces

The compiler treats `Iterator`, `Countable`, and `ArrayAccess` as built-in interfaces. User classes that `implements Iterator` must provide `rewind`, `valid`, `current`, `key`, `next`. `implements Countable` requires a `count(): int` method. `implements ArrayAccess` requires `offsetExists`, `offsetGet`, `offsetSet`, `offsetUnset`.

These names map to the same PHP interfaces — no special compiler treatment beyond declaring them as known names.

## Data structures

The four classes below are full PHP classes synthesised by the compiler. They behave like classes the user could have written by hand — type checking, dispatch, exceptions, inheritance, all work normally.

### SplStack — LIFO

```php
$s = new SplStack();
$s->push("a");
$s->push("b");
$s->push("c");
echo $s->pop();    // "c"
echo $s->pop();    // "b"
echo $s->count();  // 1
```

Methods: `push($v): void`, `pop(): mixed`, `top(): mixed`, `count(): int`, `isEmpty(): bool` plus the `Iterator` interface (`rewind` / `valid` / `current` / `key` / `next`). Iteration walks top-to-bottom (the most recent `push` is yielded first).

### SplQueue — FIFO

```php
$q = new SplQueue();
$q->enqueue("alice");
$q->enqueue("bob");
echo $q->dequeue();   // "alice"
echo $q->count();     // 1
```

Methods: `enqueue($v): void`, `dequeue(): mixed`, `count(): int`, `isEmpty(): bool` plus `Iterator` (head-to-tail).

### SplDoublyLinkedList — both ends

```php
$d = new SplDoublyLinkedList();
$d->push("middle");
$d->unshift("head");
$d->push("tail");
echo $d->bottom();  // "head"
echo $d->top();     // "tail"
echo $d->shift();   // "head"
```

Methods: `push`, `pop`, `unshift`, `shift`, `top`, `bottom`, `count`, `isEmpty`, plus the full bidirectional `Iterator` (`rewind`, `valid`, `current`, `key`, `next`, `prev`).

### SplFixedArray — bounded array

```php
$fa = new SplFixedArray(3);
$fa->offsetSet(0, "a");
$fa->offsetSet(1, "b");
$fa->offsetSet(2, "c");
echo $fa->getSize();        // 3
echo $fa->offsetGet(1);     // "b"

try {
    $fa->offsetGet(99);
} catch (RuntimeException $e) {
    echo $e->getMessage();  // "SplFixedArray index out of range"
}
```

Methods: `__construct(int $size = 0)`, `getSize(): int`, `count(): int`, `offsetExists`, `offsetGet`, `offsetSet`, `offsetUnset`, plus `Iterator`. Out-of-range `offsetGet` / `offsetSet` throws `RuntimeException`.

## Compatibility notes

- **Bracket access `$ds[$i]` is not supported.** PHP's `ArrayAccess` interface uses `[]` syntax to dispatch through `offsetGet` / `offsetSet`; elephc currently requires the explicit method calls. Use `$fa->offsetGet($i)` and `$fa->offsetSet($i, $v)`.
- **Heterogeneous element types round-trip best for scalars.** Storing `int`, `float`, `bool`, `string`, or `null` and reading back works. Storing an object value and recovering it requires the user to carry the static type at the call site (`$obj = $stack->pop(); /* $obj is mixed */`).
- **`SplFixedArray` cannot be resized.** PHP's `setSize()` is not implemented; allocate with the right size up front.
- **No `SplPriorityQueue` / `SplHeap` / `SplObjectStorage` yet.** These are tracked for a later phase of the SPL rollout.
