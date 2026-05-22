---
title: "SPL"
description: "Standard PHP Library interfaces, exceptions, and runtime-backed container classes."
sidebar:
  order: 9
---

elephc ships the SPL pieces that are needed by supported PHP code today:
iterator/counting/access interfaces, the SPL exception hierarchy, autoload and
introspection helpers, and the Phase 4 container classes
`SplDoublyLinkedList`, `SplStack`, `SplQueue`, and `SplFixedArray`.

SPL names live in the global namespace, matching PHP. They are available
without imports or runtime extensions.

## Interfaces

The compiler injects these SPL-related interfaces:

| Interface | Notes |
|---|---|
| `Traversable` | Marker interface for iterable objects |
| `Iterator` | Requires `current()`, `key()`, `next()`, `valid()`, `rewind()` |
| `IteratorAggregate` | Requires `getIterator(): Traversable` |
| `OuterIterator` | Extends `Iterator` with `getInnerIterator()` |
| `RecursiveIterator` | Extends `Iterator` with recursive traversal hooks |
| `SeekableIterator` | Extends `Iterator` with `seek()` |
| `Countable` | `count($obj)` dispatches to `count()` |
| `ArrayAccess` | Subscript syntax dispatches to offset methods |
| `SplObserver`, `SplSubject` | Observer/subject contracts |

The full interface method signatures are listed in [Classes](classes.md).

## Exceptions

The SPL exception hierarchy is built in, so user code can throw and catch these
types directly:

| Parent | Built-in subclasses |
|---|---|
| `LogicException` | `BadFunctionCallException`, `BadMethodCallException`, `DomainException`, `InvalidArgumentException`, `LengthException`, `OutOfRangeException` |
| `RuntimeException` | `OutOfBoundsException`, `OverflowException`, `RangeException`, `UnderflowException`, `UnexpectedValueException` |

They inherit the standard `Exception` constructor and `Throwable` API. Catch
matching follows PHP's normal class hierarchy rules.

## Container Classes

The Phase 4 SPL containers are built-in classes with runtime-backed storage,
not userland PHP shims:

| Class | Parent | Interfaces |
|---|---|---|
| `SplDoublyLinkedList` | - | `Iterator`, `Countable`, `ArrayAccess` |
| `SplStack` | `SplDoublyLinkedList` | inherited from parent |
| `SplQueue` | `SplDoublyLinkedList` | inherited from parent |
| `SplFixedArray` | - | `ArrayAccess`, `Countable`, `JsonSerializable` |

Container slots store `mixed`, so scalar and object values can be mixed in the
same container. Runtime ownership is handled by the SPL helpers, including
cleanup when the owning object is released.

### SplDoublyLinkedList

Supported methods:

| Method | Notes |
|---|---|
| `push(mixed $value): void` | Append to the tail |
| `pop(): mixed` | Remove from the tail |
| `unshift(mixed $value): void` | Insert at the head |
| `shift(): mixed` | Remove from the head |
| `add(int $index, mixed $value): void` | Insert at an index |
| `top(): mixed` | Read the tail |
| `bottom(): mixed` | Read the head |
| `count(): int` | Number of stored values |
| `isEmpty(): bool` | Whether the list is empty |
| `setIteratorMode(int $mode): int` | Set iterator flags |
| `getIteratorMode(): int` | Read iterator flags |
| `rewind()`, `current()`, `key()`, `next()`, `prev()`, `valid()` | Iterator operations |
| `offsetExists()`, `offsetGet()`, `offsetSet()`, `offsetUnset()` | `ArrayAccess` backing |

Supported constants:

| Constant | Value |
|---|---:|
| `IT_MODE_FIFO` | `0` |
| `IT_MODE_LIFO` | `2` |
| `IT_MODE_DELETE` | `1` |
| `IT_MODE_KEEP` | `0` |

```php
<?php
$list = new SplDoublyLinkedList();
$list->push("a");
$list->push(2);
$list[] = "c";

$list->setIteratorMode(SplDoublyLinkedList::IT_MODE_LIFO);

foreach ($list as $index => $value) {
    echo $index;
    echo ":";
    echo $value;
    echo "\n";
}
```

`IT_MODE_FIFO`, `IT_MODE_LIFO`, and `IT_MODE_DELETE` are honored during
iteration.

### SplStack

`SplStack` extends `SplDoublyLinkedList` and uses the same runtime storage. It
inherits the list methods and constants, with stack-style LIFO usage:

```php
<?php
$stack = new SplStack();
$stack->push("first");
$stack->push("second");

echo $stack->pop();   // second
echo $stack->top();   // first
```

### SplQueue

`SplQueue` extends `SplDoublyLinkedList` and adds queue aliases:

| Method | Backing behavior |
|---|---|
| `enqueue(mixed $value): void` | Same storage path as `push()` |
| `dequeue(): mixed` | Same storage path as `shift()` |

```php
<?php
$queue = new SplQueue();
$queue->enqueue("first");
$queue->enqueue("second");

echo $queue->dequeue(); // first
```

### SplFixedArray

Supported methods:

| Method | Notes |
|---|---|
| `__construct(int $size = 0)` | Allocate fixed-size storage |
| `count(): int` | Current size |
| `getSize(): int` | Current size |
| `setSize(int $size): void` | Resize storage |
| `offsetExists(mixed $index): bool` | False for invalid, unset, or null slots |
| `offsetGet(mixed $index): mixed` | Reads invalid or unset slots as `null` |
| `offsetSet(mixed $index, mixed $value): void` | Writes valid integer offsets |
| `offsetUnset(mixed $index): void` | Resets a valid slot to `null` |
| `toArray(): array` | Returns an indexed array copy |
| `jsonSerialize(): array` | Returns the same array shape as `toArray()` |

```php
<?php
$fixed = new SplFixedArray(2);
$fixed[0] = "left";
$fixed[1] = "right";

echo $fixed->getSize();
echo $fixed[0];

$fixed->setSize(3);
$fixed[2] = "tail";
```

## Autoload and Introspection

SPL autoload and class-introspection helpers are documented in
[Namespaces](namespaces.md). This includes `spl_autoload_register()`,
`spl_autoload_unregister()`, `spl_autoload_functions()`,
`spl_autoload_extensions()`, `spl_autoload_call()`, `spl_classes()`,
`spl_object_id()`, and `spl_object_hash()`.

## Compatibility Gaps

The Phase 4 containers intentionally expose only methods that have runtime
backing. Serialization/debug hooks such as `serialize()`, `unserialize()`,
`__serialize()`, `__unserialize()`, and `__debugInfo()` are not available yet.

`SplFixedArray::fromArray()` and `SplFixedArray::getIterator()` are also
deferred. `getIterator()` will make sense once the Phase 5 iterator decorator
types land.

Some edge-case exceptions are not PHP-exact yet. Invalid offsets and empty
container reads are currently conservative runtime behaviors rather than a full
replica of PHP's `RuntimeException`, `OutOfRangeException`, and `ValueError`
surface.
