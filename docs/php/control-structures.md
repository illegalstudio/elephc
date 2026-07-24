---
title: "Control Structures"
description: "if/else, while, for, foreach, switch, match, try/catch, and more."
sidebar:
  order: 3
---

## declare

`declare(strict_types=1);` is accepted at the top of a file. elephc compiles a
statically-typed subset and is **always strict**, so the directive is parsed and
treated as a no-op rather than toggling a runtime mode. The `ticks` and `encoding`
directives are likewise accepted and ignored. Directive values must be PHP
literals; `strict_types` must be the first statement, use the statement form, and
have the integer value `0` or `1`.

```php
<?php

declare(strict_types=1);

echo "always strict";
```

The block form runs its body in the enclosing scope:

```php
<?php

declare(ticks=1) {
    echo "ok";
}
```

PHP's single-statement and alternative block forms are also accepted:

```php
<?php

declare(ticks=1) echo "single statement";

declare(ticks=1):
    echo "alternative syntax";
enddeclare;
```

## if / elseif / else

```php
<?php
if ($x > 0) {
    echo "positive";
} elseif ($x < 0) {
    echo "negative";
} else {
    echo "zero";
}
```

## while

```php
<?php
$i = 0;
while ($i < 10) {
    echo $i;
    $i++;
}
```

## do...while

```php
<?php
$i = 0;
do {
    $i++;
} while ($i < 10);
```

## for

```php
<?php
for ($i = 0; $i < 10; $i++) {
    echo $i;
}
```

## foreach

```php
<?php
$arr = [1, 2, 3];
foreach ($arr as $value) {
    echo $value . "\n";
}

// With key binding (indexed arrays)
foreach ($arr as $i => $value) {
    echo "$i: $value\n";
}

// With key binding (associative arrays)
$map = ["name" => "Alice", "age" => "30"];
foreach ($map as $key => $value) {
    echo "$key = $value\n";
}

// By-reference value binding mutates the source array element.
$nums = [1, 2, 3];
foreach ($nums as &$value) {
    $value *= 2;
}
```

Use `foreach ($arr as $key => &$value)` when both the key and a mutable
element reference are needed. The key itself cannot be bound by reference.
By-reference value binding is currently supported only for array sources;
`foreach ($iterator as &$value)` over `Iterator`, `IteratorAggregate`, or
`iterable`-typed values is rejected at compile time. Use an array source or
iterate by value when consuming Traversable objects.

Untyped, `mixed`, and union-typed sources are dispatched at runtime. If the
runtime value is an indexed or associative array, both by-value and by-reference
value binding are supported. If the runtime value is an `Iterator` or
`IteratorAggregate`, it is iterated by value; by-reference value binding over
Traversable objects is rejected. Non-iterable runtime values produce a fatal
diagnostic.

`foreach` also accepts any object that implements the built-in `Iterator`
interface (`current`, `key`, `next`, `valid`, `rewind`) or the
`IteratorAggregate` interface (`getIterator(): Traversable`):

```php
<?php
class Range implements Iterator {
    private int $current;
    private int $end;
    public function __construct(int $start, int $end) {
        $this->current = $start;
        $this->end = $end;
    }
    public function rewind(): void {}
    public function valid(): bool { return $this->current < $this->end; }
    public function current(): mixed { return $this->current; }
    public function key(): mixed { return $this->current; }
    public function next(): void { $this->current = $this->current + 1; }
}

foreach (new Range(0, 5) as $i) {
    echo $i;
}
```

The loop calls `rewind()` once, then on each iteration: `valid()` to test
continuation, `current()` and `key()` to bind the loop variables, and
`next()` after the body. Method dispatch uses class vtables for concrete
iterator classes and interface metadata for `Iterator`/`IteratorAggregate`
typed values. The `iterable` pseudo-type accepts arrays and these Traversable
objects.

## break / continue

```php
<?php
for ($i = 0; $i < 100; $i++) {
    if ($i == 5) { break; }
    if ($i % 2 == 0) { continue; }
    echo $i . " ";
}
// Output: 1 3
```

Multi-level exits are supported with positive integer literal depths:

```php
<?php
for ($row = 0; $row < 3; $row++) {
    for ($col = 0; $col < 3; $col++) {
        if ($row == 1 && $col == 1) {
            break 2;       // leaves both loops
        }
    }
}
```

The level counts enclosing loops and `switch` statements, matching PHP. `break;`
and `continue;` are equivalent to `break 1;` and `continue 1;`.
Inside a `finally` block, `break` and `continue` may only target loops or
switches created inside that same `finally`; jumping out of `finally` is
rejected, matching PHP.

## switch / case / default

Standard PHP switch with fall-through semantics. Use `break` to prevent fall-through.

```php
<?php
$x = 2;
switch ($x) {
    case 1:
        echo "one";
        break;
    case 2:
        echo "two";
        break;
    default:
        echo "other";
        break;
}
```

Fall-through example:

```php
<?php
$x = 1;
switch ($x) {
    case 1:
    case 2:
        echo "one or two";
        break;
    default:
        echo "other";
}
```

## match expression

PHP 8 style match. No fall-through, returns a value, uses strict comparison (`===`).

```php
<?php
$x = 2;
$result = match($x) {
    1 => "one",
    2 => "two",
    3 => "three",
    default => "other",
};
echo $result; // two
```

If no arm matches and there is no `default`, elephc aborts with a fatal runtime error.
That implicit path does not currently construct a catchable `UnhandledMatchError`;
the builtin class is available for explicit `new`, `throw`, `catch`, and `instanceof`
expressions.

Arms may produce values of different types (objects, arrays, strings, ints, `null`),
and an arm may be a `throw` expression. When the arm types are heterogeneous, the
result is stored as a boxed `mixed` value and each value-producing arm keeps its
own runtime type, matching PHP; a `null` arm keeps the merged result nullable, so
returning such a match from a function with an inferred return type preserves the
null. Exception: arms whose types share one runtime representation (two array
types with different element types, or `int` and `bool`) merge to that
representation, which can change an arm value's observable type — see
[Known incompatibilities with PHP](types.md#known-incompatibilities-with-php).

## try / catch / finally / throw

```php
<?php
class DivisionByZeroException extends Exception {}

function divide($left, $right) {
    if ($right == 0) {
        throw new DivisionByZeroException();
    }
    return intdiv($left, $right);
}

try {
    echo divide(10, 2) . PHP_EOL;
    echo divide(10, 0) . PHP_EOL;
} catch (DivisionByZeroException $e) {
    echo "caught" . PHP_EOL;
} finally {
    echo "cleanup" . PHP_EOL;
}
```

Supported subset:

- built-in `Error` and `Exception` classes and the `Throwable` interface are available without declaring them
- `Error` and `Exception` provide `$message`, `$code`, `$previous`, `__construct($message = "", $code = 0, $previous = null)`, and the standard `Throwable` methods: `getMessage()`, `getCode()`, `getFile()`, `getLine()`, `getTrace()`, `getTraceAsString()`, `getPrevious()`, and `__toString()`
- the SPL exception hierarchy is built-in: `LogicException`, `BadFunctionCallException`, `BadMethodCallException`, `DomainException`, `InvalidArgumentException`, `LengthException`, `OutOfRangeException`, `RuntimeException`, `OutOfBoundsException`, `OverflowException`, `RangeException`, `UnderflowException`, `UnexpectedValueException`. Each is a marker subclass that inherits the constructor, `$message`, and the standard `Throwable` methods from `Exception`. Catch a specific type (`InvalidArgumentException`), an intermediate parent (`LogicException`), or the root (`Exception`/`Throwable`)
- `throw <expr>;` where `<expr>` has an object type implementing `Throwable`
- `throw <expr>` can also be used inside expressions such as `??` and ternaries
- `catch (ClassName $e)` and `catch (TypeA | TypeB $e)` for multi-catch
- `catch (Exception)`, `catch (Error)`, or another throwable type without binding the exception variable
- catch types must extend or implement `Throwable`
- user classes cannot implement `Throwable` directly; extend `Exception` or `Error` instead, or implement a user interface that extends `Throwable` from one of those subclasses
- multiple `catch` clauses
- `try { ... } finally { ... }`
- `return`, `break`, and `continue` run enclosing `finally` blocks before leaving
- `break` and `continue` written inside a `finally` block cannot target an outer loop or `switch`
