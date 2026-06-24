---
title: "Control Structures"
description: "if/else, while, for, foreach, switch, match, try/catch, and more."
sidebar:
  order: 3
---

## Expression statements

Any expression can stand alone as a statement, followed by a semicolon — not only
assignments and function calls. A statement may begin with a literal, a comparison, or a
unary operator:

```php
<?php
$total + 1;      // evaluated, result discarded
new Logger();    // constructed for its constructor's side effect
-$balance;       // unary expression statement
```

This enables the **short-circuit guard** idiom, where the right-hand side runs only when the
left-hand condition allows it:

```php
<?php
// `&&` runs the action only when the condition is true:
0 > $t && $t += 0x40;            // add 0x40 only when $t is negative

// `||` runs the action only when the condition is false:
0 === $count || printf("%d items\n", $count);
```

Both `&&` and `||` short-circuit: the right operand is not evaluated when the left already
decides the result, so any side effect (assignment, call) on the right is conditional.

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

The init and update sections may contain several comma-separated expressions, all evaluated in
order (the init list once, the update list after each iteration):

```php
<?php
for ($i = 0, $j = 10; $i < 5; $i++, $j--) {
    echo "$i:$j ";
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

The value position can also be an array-destructuring pattern (PHP 7.1+). Each
element is unpacked into the pattern targets on every iteration:

```php
<?php
foreach ([["a", "b"], ["c", "d"]] as [$x, $y]) {
    echo $x . $y; // abcd
}

foreach ([["id" => 1, "name" => "Ada"]] as ["id" => $id, "name" => $name]) {
    echo $id . ':' . $name; // 1:Ada
}

foreach (["k" => [1, 2]] as $key => [$m, $n]) {
    echo $key . $m . $n; // k12
}
```

Destructuring `foreach` patterns bind by value; by-reference pattern targets
are not supported. See [Array destructuring](arrays.md#array-destructuring) for
the full pattern grammar (positional, keyed, nested, holes).

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

## goto

`goto` transfers control to a labelled statement. A label is an identifier
followed by a colon (`name:`). Both the `goto` and its target label must live in
the same function (or both at the top level); labels are scoped per function, so
the same label name may be reused in different functions.

```php
<?php
// Break out of nested loops in one jump.
foreach ($grid as $row => $cells) {
    foreach ($cells as $col => $value) {
        if ($value === $needle) {
            goto found;
        }
    }
}
found:
echo "done\n";
```

A common use is jumping forward to shared recovery or cleanup code — for example
from inside a `catch` block to a label later in the function:

```php
<?php
function describe($thing): string {
    if ($thing === null) {
        try {
            throw new InvalidArgumentException("missing value");
        } catch (InvalidArgumentException $e) {
            $thing = "default";
            goto method_check;     // forward jump out of the try/catch
        }
    }

    method_check:
    return "describing: " . $thing;
}
```

A backward `goto` to an earlier label forms a loop, and the code that a `goto`
skips over is simply not executed.

As in PHP, `goto` cannot jump *into* a loop or `switch` body, and the target
label must exist in the same scope. A `goto` to an undefined label and a label
defined twice in the same scope are both reported as compile errors. Jumping out
of a `try` block runs that block's pending `finally`, the same as `break`,
`continue`, and `return`.

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
- `Error` and `Exception` provide `$message`, `$code`, `__construct($message = "", $code = 0)`, and the standard `Throwable` methods: `getMessage()`, `getCode()`, `getFile()`, `getLine()`, `getTrace()`, `getTraceAsString()`, `getPrevious()`, and `__toString()`
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
