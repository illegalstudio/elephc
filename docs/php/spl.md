---
title: "SPL"
description: "Standard PHP Library interfaces, exceptions, and runtime-backed container classes."
sidebar:
  order: 9
---

elephc ships the SPL pieces that are needed by supported PHP code today:
iterator/counting/access interfaces, the SPL exception hierarchy, autoload and
introspection helpers, the Phase 4 container classes, and the Phase 5 storage
iterator/decorator foundations: `EmptyIterator`, `InternalIterator`, `ArrayIterator`,
`ArrayObject`, `IteratorIterator`, `LimitIterator`, `NoRewindIterator`, and
`InfiniteIterator`, filter/cache decorators `FilterIterator`,
`CallbackFilterIterator`, and `CachingIterator`, plus the multi-source
decorators `AppendIterator` and `MultipleIterator`, and the recursive family
`RecursiveArrayIterator`, `RecursiveFilterIterator`,
`RecursiveCallbackFilterIterator`, `RecursiveIteratorIterator`, and
`ParentIterator`.

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

The Phase 4 SPL containers and Phase 5 storage/decorator iterators are built-in classes.
`SplDoublyLinkedList`, `SplStack`, `SplQueue`, and `SplFixedArray` use dedicated
runtime storage; `ArrayIterator` and `ArrayObject` use compiler-managed
keys/values storage over boxed `mixed` cells; the iterator decorators forward to
one or more `Iterator` objects:

| Class | Parent | Interfaces |
|---|---|---|
| `SplDoublyLinkedList` | - | `Iterator`, `Countable`, `ArrayAccess` |
| `SplStack` | `SplDoublyLinkedList` | inherited from parent |
| `SplQueue` | `SplDoublyLinkedList` | inherited from parent |
| `SplFixedArray` | - | `IteratorAggregate`, `ArrayAccess`, `Countable`, `JsonSerializable` |
| `EmptyIterator` | - | `Iterator` |
| `InternalIterator` | - | `Iterator` |
| `ArrayIterator` | - | `Iterator`, `ArrayAccess`, `SeekableIterator`, `Countable` |
| `ArrayObject` | - | `IteratorAggregate`, `ArrayAccess`, `Countable` |
| `IteratorIterator` | - | `OuterIterator` |
| `LimitIterator` | `IteratorIterator` | inherited from parent |
| `NoRewindIterator` | `IteratorIterator` | inherited from parent |
| `InfiniteIterator` | `IteratorIterator` | inherited from parent |
| `FilterIterator` | `IteratorIterator` | inherited from parent |
| `CallbackFilterIterator` | `FilterIterator` | inherited from parent |
| `CachingIterator` | `IteratorIterator` | `ArrayAccess`, `Countable`, `Stringable` |
| `RecursiveArrayIterator` | `ArrayIterator` | `RecursiveIterator` |
| `RecursiveFilterIterator` | `FilterIterator` | `RecursiveIterator` |
| `RecursiveCallbackFilterIterator` | `CallbackFilterIterator` | `RecursiveIterator` |
| `RecursiveIteratorIterator` | - | `OuterIterator` |
| `ParentIterator` | `RecursiveFilterIterator` | inherited from parent |
| `AppendIterator` | `IteratorIterator` | inherited from parent |
| `MultipleIterator` | - | `Iterator` |

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
| `getIterator(): Iterator` | Returns an `InternalIterator` over live fixed-array storage |
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

### Storage iterators

Supported methods:

| Class | Methods |
|---|---|
| `EmptyIterator` | `current()`, `key()`, `next()`, `rewind()`, `valid()` |
| `ArrayIterator` | `__construct(array $array = [], int $flags = 0)`, `current()`, `key()`, `next()`, `rewind()`, `valid()`, `seek(int $offset): void`, `count(): int`, `offsetExists()`, `offsetGet()`, `offsetSet()`, `offsetUnset()`, `append()`, `getArrayCopy()` |
| `ArrayObject` | `__construct(array $array = [], int $flags = 0)`, `getIterator(): ArrayIterator`, `count(): int`, `offsetExists()`, `offsetGet()`, `offsetSet()`, `offsetUnset()`, `append()`, `getArrayCopy()` |
| `IteratorIterator` | `__construct(Traversable $iterator, ?string $class = null)`, `current()`, `key()`, `next()`, `rewind()`, `valid()`, `getInnerIterator(): ?Iterator` |
| `LimitIterator` | `__construct(Iterator $iterator, int $offset = 0, int $limit = -1)`, `rewind()`, `next()`, `valid()`, `seek(int $offset): void`, `getPosition(): int`, plus inherited forwarding methods |
| `NoRewindIterator` | `__construct(Iterator $iterator)`, `rewind()` no-op, plus inherited forwarding methods |
| `InfiniteIterator` | `__construct(Iterator $iterator)`, `next()` cycles to the start when the inner iterator is exhausted, plus inherited forwarding methods |
| `FilterIterator` | `__construct(Iterator $iterator)`, abstract `accept(): bool`, `rewind()`, `next()`, plus inherited forwarding methods |
| `CallbackFilterIterator` | `__construct(Iterator $iterator, callable $callback)`, `accept(): bool` calling the callback as `callback(current, key, inner)` |
| `CachingIterator` | `__construct(Iterator $iterator, int $flags = CachingIterator::CALL_TOSTRING)`, `rewind()`, `valid()`, `next()`, `current()`, `key()`, `hasNext()`, `__toString()`, `getFlags()`, `setFlags(int $flags): void`, `getCache()`, `count()`, `offsetExists()`, `offsetGet()`, `offsetSet()`, `offsetUnset()` |
| `RecursiveArrayIterator` | `__construct(array\|object $array = [], int $flags = 0)`, `hasChildren(): bool`, `getChildren(): ?RecursiveIterator`, plus inherited `ArrayIterator` methods |
| `RecursiveFilterIterator` | `__construct(RecursiveIterator $iterator)`, `hasChildren(): bool`, `getChildren(): ?RecursiveIterator`, plus inherited `FilterIterator` methods |
| `RecursiveCallbackFilterIterator` | `__construct(RecursiveIterator $iterator, callable $callback)`, `hasChildren(): bool`, `getChildren(): ?RecursiveIterator`, plus inherited callback filtering |
| `RecursiveIteratorIterator` | `__construct(RecursiveIterator $iterator, int $mode = RecursiveIteratorIterator::LEAVES_ONLY, int $flags = 0)`, `rewind()`, `valid()`, `current()`, `key()`, `next()`, `getDepth(): int`, `getInnerIterator(): ?Iterator`, `getSubIterator(int $level = -1): ?RecursiveIterator` |
| `ParentIterator` | `__construct(RecursiveIterator $iterator)`, `accept(): bool`, `getChildren(): ?RecursiveIterator`, plus inherited recursive filtering |
| `AppendIterator` | `__construct()`, `append(Iterator $iterator): void`, `rewind()`, `valid()`, `current()`, `key()`, `next()`, `getInnerIterator(): ?Iterator`, `getIteratorIndex(): int\|string\|null`, `getArrayIterator(): ArrayIterator` |
| `MultipleIterator` | `__construct(int $flags = MultipleIterator::MIT_NEED_ALL)`, `attachIterator(Iterator $iterator, string\|int\|null $info = null): void`, `detachIterator(Iterator $iterator): void`, `containsIterator(Iterator $iterator): bool`, `countIterators(): int`, `getFlags(): int`, `setFlags(int $flags): void`, `rewind()`, `valid()`, `key()`, `current()`, `next()` |

```php
<?php
$it = new ArrayIterator(["a" => 10, "b" => 20]);
$it["c"] = 30;

foreach ($it as $key => $value) {
    echo $key;
    echo "=";
    echo $value;
    echo "\n";
}

$obj = new ArrayObject(["left" => 1, "right" => 2]);
foreach ($obj as $key => $value) {
    echo $key;
    echo $value;
}

$wrapped = new IteratorIterator($obj, "ArrayObject");
foreach ($wrapped as $key => $value) {
    echo $key;
    echo $value;
}

$limited = new LimitIterator(
    new InfiniteIterator(new ArrayIterator([1, 2])),
    0,
    5
);
foreach ($limited as $value) {
    echo $value; // 12121
}

function keep_large(int $value, string $key, Iterator $inner): bool {
    return $value > 1;
}

$filter = new CallbackFilterIterator(
    new ArrayIterator(["a" => 1, "b" => 2]),
    keep_large(...)
);
foreach ($filter as $key => $value) {
    echo $key;
    echo $value;
}

$cache = new CachingIterator(
    new ArrayIterator(["a" => "A", "b" => "B"]),
    CachingIterator::FULL_CACHE | CachingIterator::TOSTRING_USE_KEY
);
foreach ($cache as $key => $value) {
    echo (string) $cache;
    echo $cache->hasNext() ? "more" : "last";
}
echo $cache["a"];

$append = new AppendIterator();
$append->append(new ArrayIterator(["a" => 1]));
$append->append(new ArrayIterator(["b" => 2]));
foreach ($append as $key => $value) {
    echo $key;
    echo $value;
}

$multi = new MultipleIterator(
    MultipleIterator::MIT_NEED_ANY | MultipleIterator::MIT_KEYS_ASSOC
);
$multi->attachIterator(new ArrayIterator(["a" => 1, "b" => 2]), "left");
$multi->attachIterator(new ArrayIterator(["x" => 10]), "right");
foreach ($multi as $keys => $values) {
    echo $keys["left"];
    echo is_null($values["right"]) ? "missing" : $values["right"];
}

$tree = new RecursiveIteratorIterator(
    new RecursiveArrayIterator(["a" => ["x" => 1], "b" => 2]),
    RecursiveIteratorIterator::SELF_FIRST
);
foreach ($tree as $key => $value) {
    echo $tree->getDepth();
    echo ":";
    echo $key;
    echo "=";
    echo gettype($value) === "array" ? "array" : $value;
}
```

`IteratorIterator` accepts PHP's optional `$class` downcast argument. Direct
`Iterator` inputs evaluate the argument and ignore it. `IteratorAggregate`
inputs validate that the class string names the aggregate class or one of its
concrete Traversable base classes before calling `getIterator()`.

`ArrayIterator` and `ArrayObject` preserve insertion-order keys for array
inputs and for writes through `ArrayAccess`. Appends use the current storage
length as the next integer key. `IteratorIterator` accepts any `Traversable`;
when passed an `IteratorAggregate`, it calls `getIterator()` once and wraps the
returned iterator. `LimitIterator`, `NoRewindIterator`, and `InfiniteIterator`
follow PHP's constructors and require an `Iterator` directly.

`FilterIterator` is abstract and skips inner positions whose `accept()` returns
false during `rewind()` and `next()`. `CallbackFilterIterator` stores a callable
and invokes it with current value, current key, and the inner iterator object;
closure and first-class-callable capture environments are preserved with the
iterator object.
`CachingIterator` implements one-element lookahead through `hasNext()`, supports
the string mode flags `CALL_TOSTRING`, `TOSTRING_USE_KEY`,
`TOSTRING_USE_CURRENT`, and `TOSTRING_USE_INNER`, and supports `FULL_CACHE` for
`getCache()`, `count()`, and `ArrayAccess`.

`AppendIterator` skips exhausted appended iterators and exposes the current
storage key through `getIteratorIndex()`. Its `getArrayIterator()` result is a
live `ArrayIterator` view: appending, keyed `offsetSet()`, and `offsetUnset()`
through that view updates the owner. `MultipleIterator` supports PHP's
`MIT_NEED_ANY`, `MIT_NEED_ALL`, `MIT_KEYS_NUMERIC`, and `MIT_KEYS_ASSOC` flags.
Re-attaching the same iterator updates its info instead of duplicating it. When
associative-key mode is active, attaching an iterator with `null` info raises
`InvalidArgumentException` when `key()` or `current()` materializes the
composite arrays, matching PHP.

`RecursiveArrayIterator` detects nested arrays and nested `RecursiveIterator`
objects through `hasChildren()`. `RecursiveIteratorIterator` supports
`LEAVES_ONLY`, `SELF_FIRST`, and `CHILD_FIRST`; it keeps a live stack of source
sub-iterators so `getDepth()`, `getInnerIterator()`, and `getSubIterator()`
track the active cursors. `RecursiveCallbackFilterIterator` preserves closure
and first-class-callable capture environments when it wraps child iterators.
`ParentIterator` recursively keeps only entries that have children.

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
callbacks. If the call site has no single static callback signature, elephc can
dispatch dynamic indexed or associative args by matching the runtime callable
pointer against user functions and closure/FCC wrappers available in that
codegen context, then applying the matched target's parameter and return
metadata. Runtime string callback names dispatch over user functions by
case-insensitive name matching and then use the same metadata path. For variadic
callbacks, named keys consumed by fixed parameters are not copied into
`...$rest`; remaining string keys keep their names, and remaining numeric keys
are reindexed from zero. Literal arrays with expressions are evaluated once
before iteration starts. Dynamic arrays passed to by-reference callback
parameters use temporary reference cells, so callback writes do not mutate the
source argument array.
