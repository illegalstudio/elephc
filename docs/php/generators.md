---
title: "Generators"
description: "Generator functions with yield, the built-in Iterator and Generator types, foreach over Iterator objects, and Generator::send for coroutine-style flow."
sidebar:
  order: 15
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

The runtime stores the inner generator pointer in the outer frame's
`delegated_iter` slot and reuses one resume state index for every step
of the delegation. v1 only delegates to function calls returning
`Generator` and locals that hold a `Generator`; arbitrary `Iterator`
expressions in `yield from` are not yet supported. Invalid non-generator
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

`$g->throw($exc)` injects an exception that propagates up the caller's
stack as if the generator had thrown it. The generator is marked
terminated so subsequent calls become no-ops. Since `try`/`catch`
inside a generator body is rejected at type-check time, the exception
always lands in the caller's nearest active handler:

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

Generator bodies in elephc can contain ordinary local variables, simple
arithmetic, and the usual control-flow constructs. Local int variables
declared inside the generator survive across yield points — the resume
function reads/writes the same heap-backed slot on every entry.

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

Supported in v1: `if`/`else`/`elseif`, `while`, `do-while`, `for`,
`break`, `continue`, `switch` over int subjects with integer-literal
cases (with PHP fall-through semantics), and arbitrary nesting of all
of the above. Comparison operators include `<`, `<=`, `>`, `>=`, `==`,
`!=`, `===`, and `!==`. Arithmetic supports `+`, `-`, `*`, and integer
`/` (signed division).

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

## Restrictions in v1

Generator bodies are translated to a state machine at compile time.
The translation only recognizes the subset of PHP constructs listed
above; anything outside that grammar makes the generator silently stop
yielding past the unsupported statement. The compiler does not produce
an error in that case so that complex generators can be ported
incrementally.

The following are **not yet supported** inside generator bodies:

- `try` / `catch` / `finally` (rejected at type-check time — yield
  inside an exception scope is explicitly disallowed).
- `foreach` over an `Iterator`-typed parameter when the static type is
  the interface itself (concrete classes implementing Iterator work
  fine; interface-typed parameters need interface-vtable dispatch
  which v1 doesn't model).
- `Generator::throw()` re-thrown **into** a generator body for the
  body's own try/catch to handle (since try/catch is forbidden inside
  the body, the runtime always propagates straight to the caller's
  catch).
- `yield from` over an `Iterator` interface instance whose static
  class is unknown (only `yield from <generator_function(args)>` and
  `yield from $local` where the local holds a Generator pointer work).
- Fiber suspension/resume operations inside generator bodies are outside
  the v1 generator-body lowering subset.

Generator bodies *do* support Mixed-typed locals: `$msg = "hello";
yield $msg;` works, with the resume function reading/writing the
boxed cell pointer in the same frame slot across yield points.

The full list of remaining work for generators lives in `ROADMAP.md`.

## How it works at runtime

Each generator function `f` produces two target-specific symbols:

- `_fn_<f>` — the wrapper that allocates a `GeneratorFrame` on the
  heap (fixed 80-byte header followed by N×8-byte slots for parameters
  and int locals), stamps it with `Generator`'s class id, and returns
  the frame pointer.
- `_fn_<f>__resume` — the resume function that dispatches on the
  frame's `state_idx` to either the body's entry point (state 0) or one
  of the per-yield resume labels.

Each `yield` site receives a unique state index. At a yield, the
resume function:

1. Calls `__rt_mixed_from_value` to box the yielded payload (and key)
   into a Mixed cell.
2. Refcount-drops the previous boxed key/value via `__rt_decref_mixed`
   so generators don't leak a cell per iteration.
3. Stores the new Mixed pointer into the frame's `last_key` /
   `last_value` slot.
4. Sets `state_idx` to the next-yield index and returns.

The synthetic `Generator` class has no PHP body — its method dispatch is
intercepted in the codegen and routed directly to the `__rt_gen_*`
runtime helpers (`current`, `key`, `valid`, `next`, `send`, `rewind`,
`throw`, `getReturn`).
