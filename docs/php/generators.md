---
title: "Generators"
description: "Generator functions with yield, the built-in Iterator and Generator types, foreach over Iterator objects, and Generator::send for coroutine-style flow."
sidebar:
  order: 16
---

A *generator* is a function whose body uses the `yield` keyword. Calling a
generator function returns a `Generator` object — a real PHP object that
implements the built-in `Iterator` interface — instead of executing the
body. Each call to `Generator::next()` (and the implicit calls inside a
`foreach`) runs the body up to the next `yield`, hands the yielded value
back, and suspends until the next call.

## Quick example

```php
<?php
function counter(int $from) {
    $i = $from;
    while ($i < $from + 3) {
        yield $i;
        $i++;
    }
}

foreach (counter(10) as $v) {
    echo $v;
    echo " ";
}
// Prints: 10 11 12
```

## yield with explicit and auto keys

When `yield` is used without an explicit key, PHP assigns an
auto-incrementing integer key starting at 0:

```php
<?php
function gen() { yield "a"; yield "b"; yield "c"; }
foreach (gen() as $k => $v) {
    echo "$k=$v ";
}
// Prints: 0=a 1=b 2=c
```

Explicit keys are passed through `=>`. Keys can be ints or string literals
and do not bump the auto counter:

```php
<?php
function gen() {
    yield "header";       // auto-key 0
    yield "k" => 42;      // explicit key — counter unchanged
    yield "footer";       // auto-key 1
}
```

## yield from

`yield from <array_literal>` expands at compile time to one yield per
element. Useful for sandwiching a fixed sequence between dynamic yields:

```php
<?php
function delegate() {
    yield 0;
    yield from [10, 20, 30];
    yield 99;
}
foreach (delegate() as $v) { echo $v . " "; }
// Prints: 0 10 20 30 99
```

`yield from <generator_function(args)>` delegates iteration to another
generator at runtime. The outer generator forwards each value (and key)
from the inner, then continues its own body once the inner is exhausted:

```php
<?php
function inner() { yield 1; yield 2; yield 3; }
function outer() {
    yield 0;
    yield from inner();
    yield 99;
}
foreach (outer() as $v) { echo $v . " "; }
// Prints: 0 1 2 3 99
```

`yield from <generator>` is driven by the `__rt_gen_delegate` runtime
helper, which runs on the outer generator's coroutine stack: it advances
the inner generator, re-yields each inner key/value through the outer
suspend boundary, forwards sent values into the inner generator, and
returns the inner generator's `getReturn()`. `yield from <array>` is
desugared into an iterator loop. Invalid non-generator, non-iterable
delegates are rejected at type-check time.

Like PHP, `yield from` also evaluates to the delegated generator's
terminal `return` value, so the outer generator can capture and yield or
return it after delegation finishes:

```php
<?php
function inner() {
    yield 1;
    return 42;
}

function outer() {
    $ret = yield from inner();
    yield $ret;
}

foreach (outer() as $v) { echo $v . " "; }
// Prints: 1 42
```

The delegated return value can also become the outer generator's terminal
return value directly:

```php
<?php
function outer() {
    return yield from inner();
}
```

## Generator closures

Anonymous functions that contain `yield` also return `Generator`
objects. Captured scalar locals are copied into the generator frame just
like ordinary closure captures:

```php
<?php
$start = 7;
$gen = function() use ($start) {
    yield $start;
    yield $start + 1;
};

foreach ($gen() as $v) { echo $v . " "; }
// Prints: 7 8
```

## return value and Generator::getReturn()

A generator body may end with `return <expr>;` to stash a final value
(distinct from yielded values) that the caller retrieves with
`Generator::getReturn()` after iteration completes:

```php
<?php
function gen() {
    yield 1;
    yield 2;
    return 42;
}

$g = gen();
foreach ($g as $v) { echo $v . " "; }
echo "ret=" . $g->getReturn();
// Prints: 1 2 ret=42
```

A bare `return;` (no value) terminates the generator without writing a
return value; `getReturn()` then surfaces the slot's initial null/0.

## Generator::throw

`$g->throw($exc)` resumes the generator and raises `$exc` **at the
suspended `yield`**, exactly like PHP. If the generator body has a
surrounding `try`/`catch`, that handler runs and execution continues; the
value of `throw()` is the next yielded value. If the exception is not
caught inside the body, it propagates to the caller as if the generator
had thrown it.

```php
<?php
function gen() {
    try {
        yield 1;
    } catch (Exception $e) {
        echo "caught inside: " . $e->getMessage() . " ";
    }
    yield 2;
}

$g = gen();
echo $g->current() . " ";              // 1
echo $g->throw(new Exception("boom")); // caught inside: boom 2
// Prints: 1 caught inside: boom 2
```

When the body has no handler, the exception escapes to the caller's
nearest active handler:

```php
<?php
function gen() {
    yield 1;
    yield 2;
}

try {
    $g = gen();
    $g->rewind();
    echo $g->current() . " ";   // 1
    $g->throw(new Exception("boom"));
    echo "unreachable";
} catch (Exception $e) {
    echo "caught: " . $e->getMessage();
}
// Prints: 1 caught: boom
```

## Locals and control flow inside generator bodies

Generator bodies in elephc can contain ordinary local variables,
arithmetic, and the usual control-flow constructs. Locals declared inside
the generator survive across yield points because the body's real stack
frame is suspended and resumed in place on the coroutine stack.

```php
<?php
function fib(int $count) {
    $a = 0;
    $b = 1;
    $i = 0;
    while ($i < $count) {
        yield $a;
        $c = $a + $b;
        $a = $b;
        $b = $c;
        $i++;
    }
}

foreach (fib(10) as $v) { echo $v . " "; }
// Prints: 0 1 1 2 3 5 8 13 21 34
```

Because the body is compiled like an ordinary function, every control-flow
construct and operator the language supports is available inside a
generator body — `if`/`else`/`elseif`, `while`, `do-while`, `for`,
`foreach`, `switch`, `break`, `continue`, `try`/`catch`/`finally`, and
arbitrary nesting — with the same semantics they have outside a generator.

## Calling user functions from a generator body

Generator bodies can invoke user functions whose return type is `int`,
with up to 8 int arguments:

```php
<?php
function helper(int $x): int { return $x * 2; }

function gen() {
    $i = 1;
    while ($i < 5) {
        yield helper($i) + 10;
        $i++;
    }
}

foreach (gen() as $v) { echo $v . " "; }
// Prints: 12 14 16 18
```

## Generator::send for coroutine-style flow

`yield` is also an expression: assigning its result to a variable lets
the caller pump values *into* the generator via `Generator::send`. The
sent value becomes the result of the in-progress yield expression.

```php
<?php
function echoer() {
    $a = yield 1;     // first yield: $a starts as null until send()
    $b = yield $a;    // yields whatever was sent in
    yield $b;
}

$g = echoer();
$g->rewind();              // runs to first yield → current() = 1
echo $g->current();        // 1
$g->send(100);             // resumes with $a = 100, runs to next yield
echo $g->current();        // 100
$g->send(200);             // resumes with $b = 200
echo $g->current();        // 200
```

If the generator is resumed via `next()` instead of `send()`, the
in-progress yield expression evaluates to `0` for an int-typed LHS
local. For Mixed-typed LHS locals (e.g. `$x = yield $prompt;` where
`$x` was previously assigned a string or array), `next()` leaves the
slot at its previous value while `send($v)` transfers the boxed Mixed
pointer into the slot:

```php
<?php
function chat() {
    $x = "init";              // $x is Mixed-typed
    $x = yield "first";        // $x ← whatever was sent (Mixed)
    yield $x;
    $x = yield "second";
    yield $x;
}

$g = chat();
$g->rewind();
echo $g->current() . " ";     // "first"
$g->send("alpha");
echo $g->current() . " ";     // "alpha" — string round-tripped
$g->send("beta");
echo $g->current() . " ";     // "second"
$g->send("gamma");
echo $g->current();           // "gamma"
```

## foreach over arbitrary Iterator and IteratorAggregate objects

`foreach` accepts any object that implements the built-in `Iterator`
interface (`current`, `key`, `next`, `valid`, `rewind`) or
`IteratorAggregate` (`getIterator(): Traversable`). Generators are one
such producer; user classes can implement either protocol:

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

foreach (new Range(0, 5) as $i) { echo $i; }
// Prints: 01234
```

The loop calls `rewind()` once, then on each iteration: `valid()` to
test continuation, `current()` and `key()` to bind the loop variables,
and `next()` after the body. Method dispatch goes through the regular
vtable.

When `foreach` is used on an `IteratorAggregate`, the codegen calls
`getIterator()` once before the loop and uses the returned object's
class for the per-iteration dispatches:

```php
<?php
class AggregateRange implements IteratorAggregate {
    public function getIterator(): Range { return new Range(0, 3); }
}

foreach (new AggregateRange() as $v) { echo $v; }
// Prints: 012
```

## Control flow inside generator bodies

A generator body runs on its own coroutine stack and is compiled by the
same backend as an ordinary function, so it supports the full language:
arbitrary control flow (`if`/`while`/`for`/`foreach`/`switch`),
`try`/`catch`/`finally` around `yield`, method calls, `return` values, and
locals of any type — all preserved across yield points because the body's
real stack frame is suspended and resumed in place.

```php
<?php
function items() {
    foreach (["a" => 1, "b" => 2] as $k => $v) {
        try {
            yield $k => $v;
        } catch (Exception $e) {
            echo "handled: " . $e->getMessage() . " ";
        }
    }
}
```

## How it works at runtime

A `Generator` is a stackful coroutine that reuses the Fiber runtime: the
object reuses the Fiber memory layout (its own mmap'd stack plus the
exception-handler and `transfer`/`pending` fields) and a small block of
generator-specific fields (`last_key`, `last_value`, `return_value`,
`auto_key`). Each generator function `f` produces three target-specific
symbols:

- `_fn_<f>` — the **constructor** at the public symbol. It allocates the
  Generator coroutine object (`__rt_fiber_construct`), boxes the call
  arguments and closure captures into the object's `start_args` slots, and
  returns the object. The body does not run yet — it starts lazily on the
  first accessor. The `start_args` storage has room for **7** boxed values,
  so a generator may declare at most 7 parameters (counting closure
  captures); a generator with more is rejected at compile time.
- `_fn_<f>__genbody` — the **body**, compiled by the normal backend as a
  Mixed-returning function. Its `return` value is what `getReturn()`
  surfaces.
- `_fn_<f>__gencb` — the **coroutine entry wrapper**. The fiber entry
  trampoline runs it on the coroutine stack; it unboxes `start_args` back
  into the body's parameter registers, runs the body, and parks the body's
  return value in the `return_value` slot.

Each `yield` lowers to `__rt_gen_suspend(key, value)`, which records the
boxed key/value into the generator's `last_key`/`last_value` slots and then
suspends the coroutine. Because the suspend boundary re-raises a scheduled
exception **inside** the coroutine's own stack, `Generator::throw()` lands
in a `try`/`catch` inside the body. `yield from <generator>` is driven by
`__rt_gen_delegate`, which forwards sent values into the inner generator and
returns its `getReturn()`; `yield from <array>` is desugared into an
iterator loop that re-yields each entry.

The synthetic `Generator` class has no PHP body — its method dispatch is
intercepted in the codegen and routed directly to the `__rt_gen_*`
runtime helpers (`current`, `key`, `valid`, `next`, `send`, `rewind`,
`throw`, `getReturn`), which drive the coroutine through the shared Fiber
primitives.
