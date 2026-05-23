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
| `setIteratorMode(int $mode): void` | Set iterator flags |
| `getIteratorMode(): int` | Read iterator flags |
| `rewind()`, `current()`, `key()`, `next()`, `prev()`, `valid()` | Iterator operations |
| `offsetExists()`, `offsetGet()`, `offsetSet()`, `offsetUnset()` | `ArrayAccess` backing |
| `serialize()`, `unserialize(string $data): void` | Legacy SPL list payload round-trip |
| `__serialize()`, `__unserialize(array $data): void` | PHP 7.4+ array state round-trip |
| `__debugInfo(): array` | Debug state with flags and list contents |

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
| `__wakeup(): void` | PHP wakeup hook |
| `fromArray(array $array, bool $preserveKeys = true): SplFixedArray` | Build a fixed array from PHP array data |
| `__serialize(): array` | Returns the same indexed values as `toArray()` |
| `__unserialize(array $data): void` | Replaces storage with packed source values |
| `count(): int` | Current size |
| `getSize(): int` | Current size |
| `setSize(int $size): void` | Resize storage |
| `offsetExists(mixed $index): bool` | False for invalid, unset, or null slots |
| `offsetGet(mixed $index): mixed` | Reads unset slots as `null`; invalid offsets throw |
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
`spl_object_id()`, `spl_object_hash()`, `class_implements()`,
`class_parents()`, and `class_uses()`.

## Iterator Helper Functions

The iterator helper functions cover the PHP SPL traversal helpers:

| Function | Signature | Notes |
|---|---|---|
| `iterator_to_array()` | `iterator_to_array(Traversable\|array $iterator, bool $preserve_keys = true): array` | Rewinds object iterators, collects `current()` values, and optionally preserves `key()` results |
| `iterator_count()` | `iterator_count(Traversable\|array $iterator): int` | Rewinds and advances object iterators until `valid()` is false |
| `iterator_apply()` | `iterator_apply(Traversable $iterator, callable $callback, ?array $args = null): int` | Calls the callback once per valid position, stops when it returns false, and returns the invocation count |

```php
<?php
class Range implements Iterator {
    private int $i = 0;

    public function rewind(): void { $this->i = 0; }
    public function valid(): bool { return $this->i < 3; }
    public function current(): int { return $this->i + 10; }
    public function key(): string { return "k" . $this->i; }
    public function next(): void { $this->i = $this->i + 1; }
}

$items = iterator_to_array(new Range());
echo iterator_count(new Range());

function tick(string $label): bool {
    echo $label;
    return true;
}

echo iterator_apply(new Range(), "tick", ["!"]);
```

AOT constraints: `iterator_to_array()` accepts literal or dynamic scalar
`preserve_keys` values and applies PHP truthiness at runtime. `iterator_apply()`
accepts statically known Traversable objects and runtime-dispatched
`Traversable` or `iterable` values; if an `iterable` value is an array at
runtime, the program aborts because PHP's `iterator_apply()` signature requires
`Traversable`. The third `iterator_apply()` argument may be omitted, `null`, a
literal array, a dynamic indexed array value, or a dynamic associative array when
the callback has a statically known signature, including userland variadic
callbacks. For variadic callbacks, named keys consumed by fixed parameters are
not copied into `...$rest`; remaining string keys keep their names, and remaining
numeric keys are reindexed from zero. Literal arrays with expressions are
evaluated once before iteration starts. Dynamic associative arrays with callback
values whose signatures are not known are rejected because PHP treats string keys
as named callback arguments. Callback expressions without statically known
signatures can receive dynamic indexed argument arrays through a generated
runtime arity dispatcher. Dynamic arrays passed to by-reference callback
parameters use temporary reference cells, so callback writes do not mutate the
source argument array.

## Compatibility Gaps

`SplFixedArray::getIterator()` is deferred because it needs the Phase 5 iterator
runtime surface (`IteratorAggregate` return objects and the iterator decorator
classes). The Phase 4 containers otherwise keep their runtime-backed method
surface aligned with PHP's empty-container, invalid-offset, serialization, and
fixed-array key behaviors.
