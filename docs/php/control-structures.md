---
title: "Control Structures"
description: "if/else, while, for, foreach, switch, match, try/catch, and more."
sidebar:
  order: 3
---

## declare

`declare(strict_types=1);` switches the file to PHP's strict parameter binding.
Like PHP, the directive is scoped to the **physical file it appears in**: it does
not propagate into files that file includes, it does not reach back into the file
that included it, and it is the file containing the *call site* — not the file
declaring the callee — that decides which rules apply.

Under the directive, a declared scalar parameter accepts only an argument of
exactly that type, plus PHP's one surviving widening of `int` into a declared
`float`. Every other conversion PHP performs in coercive mode is a compile error
naming the `TypeError` PHP would throw — see
[Types → Strict types](./types.md#strict-types) for the full table and the
surfaces the directive does and does not reach, and
[Types → Parameter type coercion](./types.md#parameter-type-coercion) for what a
file without the directive accepts.

`declare(strict_types=0);` is the explicit spelling of PHP's default and changes
nothing.

The `ticks` and `encoding` directives are accepted and ignored.
Directive values must be PHP literals; `strict_types` must be the first
statement, use the statement form, and have the integer value `0` or `1`.

```php
<?php

declare(strict_types=1);

function takesInt(int $i) { return $i; }

echo takesInt(42);        // 42
echo takesInt((int) "7"); // 7   — an explicit cast is always fine
// echo takesInt(true);   // compile error: must be of type int, bool given
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

PHP's alternative `if ($x): … elseif … else: … endif;` form is also accepted —
see [Alternative syntax](#alternative-syntax).

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

The **init** and **update** clauses take any PHP expression, not just an assignment or an
increment — an array push, an indexed or property assignment, a call — and either may be a
comma-separated list, which PHP evaluates left to right:

```php
<?php
$squares = [];
for ($i = 0, $limit = 4; $i < $limit; $i++, $squares[] = $i * $i) {
    // ...
}

$box = new Box();
for ($i = 0; $i < 4; $box->total = $i) {
    $i++;
}
```

A comma inside a call's argument list, an array literal or a closure body is **not** a clause
separator, so `for ($i = max(0, 1); …)` and a closure with its own `;` both parse as one
member:

```php
<?php
for ($fmt = function (int $n): string { return "#$n"; }, $i = 0; $i < 3; $i++) {
    echo $fmt($i);
}
```

`throw` counts as an expression here, as it does everywhere in PHP 8, so either clause
accepts it:

```php
<?php
for (throw new LogicException("unreachable"); false; ) {
}

for ($i = 0; $i < 3; throw new RuntimeException("one pass only")) {
    echo $i;
}
```

Three limits, the first two matching PHP:

- The clauses accept **expressions only**. `for (echo "x"; …)` is a parse error, as it is in
  PHP; so is a named declaration (`function f() {}`), which PHP rejects because it reads
  `function` in that position as the start of a closure.
- The **condition** clause does not yet accept a comma-separated list. PHP evaluates every
  expression there and uses the last one's value, but elephc has no sequence expression to
  hold the list and the condition re-runs each iteration, so it reports a diagnostic naming
  the limitation rather than mis-parsing.
- `include` / `require` **are** expressions and PHP does run them in a clause, but elephc
  rejects them there: include resolution expands a value-include by rewriting the surrounding
  statement *list*, which a clause is not. Move it above the loop. An include inside a
  **closure** in a clause is fine — it runs when the closure is called, not while the clause
  is evaluated.

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

The value target can also be a destructuring pattern, in either spelling, with or
without a key:

```php
$points = [[1, 2], [3, 4]];
foreach ($points as [$x, $y]) {
    echo "$x,$y\n";
}
foreach ($points as list($x, $y)) { /* same thing */ }
foreach ($points as $i => [$x, $y]) {
    echo "$i: $x,$y\n";
}

// Keyed, skipped, and nested patterns work exactly as they do in an assignment.
$rows = [["name" => "Ada", "role" => "admin"]];
foreach ($rows as ["name" => $name, "role" => $role]) {
    echo "$name is $role\n";
}
foreach ([[1, 2, 3]] as [, $second]) { echo $second; }
foreach ([[1, [2, 3]]] as [$a, [$b, $c]]) { echo $a, $b, $c; }
```

A destructuring pattern binds one element per iteration and then unpacks it, so it
follows the rules in [Array destructuring](./arrays.md): keyed and unkeyed entries
cannot be mixed, and an empty pattern (`foreach ($x as [])`) is an error. The `&`
reference marker applies to a variable target, never to a whole pattern, so
`foreach ($x as &[$a, $b])` is rejected.

Use `foreach ($arr as $key => &$value)` when both the key and a mutable
element reference are needed. The key itself cannot be bound by reference.

The source of a by-reference loop can be any array the program can name, and the
writes land in it: a local, an array element (`$a[0]`), an object property, a
property reached through a runtime name (`$o->$n`), through an object held in an
array element (`$arr[0]->x`), or through a call result (`$o->get()->x`). The loop
holds whatever storage it is iterating for as long as it runs, so replacing the
receiver inside the body does not disturb it. A receiver that turns out to be
`null` — `$arr[5]->x` on a shorter array — raises PHP's
`Attempt to modify property "x" on null` `Error`, because a by-reference source is
a write target rather than a read.

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

## Alternative syntax

`if`, `while`, `for`, `foreach`, and `switch` all accept PHP's alternative
syntax: a `:` opens the body instead of `{`, and a matching `endif;`,
`endwhile;`, `endfor;`, `endforeach;`, or `endswitch;` closes it. (`declare`
uses the same shape with `enddeclare;` — see above.)

```php
<?php
if ($x > 0):
    echo "positive";
elseif ($x < 0):
    echo "negative";
else:
    echo "zero";
endif;

while ($i < 3):
    $i++;
endwhile;

for ($i = 0; $i < 3; $i++):
    echo $i;
endfor;

foreach ([1, 2, 3] as $value):
    echo $value;
endforeach;

switch ($x):
    case 1:
        echo "one";
        break;
    default:
        echo "other";
endswitch;
```

The two forms are exactly equivalent — the alternative body compiles to the same
code as the braced one — and they nest freely in either direction, so an
alternative `if` can sit inside a braced `foreach` and vice versa.

Two rules match PHP:

- **One style per `if` chain.** Every branch of a given `if` must use the same
  form. `if ($x) { ... } else: ... endif;` is rejected, as is
  `if ($x): ... else { ... } endif;`. Note that this means `else if` (two words)
  cannot be used in an alternative chain — write `elseif`.
- **The terminator needs its semicolon.** `endif`, `endwhile`, `endfor`,
  `endforeach`, and `endswitch` are each followed by `;`.

Since elephc has no inline-HTML mode, the alternative forms are a pure
readability choice rather than a templating feature.

## goto

**`goto` is not supported.** Both the statement and its target label are
rejected at compile time:

```text
error[2:1]: `goto` is not supported: elephc compiles structured control flow
only, so a jump to an arbitrary label inside a function has no lowering.
Please restructure the jump with `break`, `continue`, a loop flag, or an early
`return`

error[4:1]: `goto` labels are not supported: the label `end:` can only be
reached by `goto`, which elephc does not support.
```

elephc analyses control flow structurally — termination and reachability
analysis, flow-sensitive type narrowing, loop and branch pruning, and constant
propagation all assume the statement tree describes the CFG. An arbitrary
intra-function jump breaks that assumption, so the construct is rejected outright
rather than partially supported. Use `break` (including `break 2;`), `continue`,
a loop flag, or an early `return` instead; those cover PHP's common `goto` use
of bailing out of nested loops.

`goto` is still a reserved word, so it cannot be used as a function name — but,
as in PHP, it remains valid as a method or constant name (`$obj->goto()`).

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
- the built-in `Error` hierarchy is available on the same terms, with PHP's exact parents:

  | class | extends |
  | --- | --- |
  | `Error` | implements `Throwable` |
  | `TypeError` | `Error` |
  | `ArgumentCountError` | `TypeError` |
  | `ValueError` | `Error` |
  | `ArithmeticError` | `Error` |
  | `DivisionByZeroError` | `ArithmeticError` |
  | `AssertionError` | `Error` |
  | `UnhandledMatchError` | `Error` |
  | `FiberError` | `Error` |

  Each is a marker subclass inheriting the constructor, `$message`, `$code`, `$previous`, and the standard `Throwable` methods from `Error`, exactly as the SPL exceptions inherit theirs from `Exception`. Catch matching walks the whole chain, so an `ArgumentCountError` is caught by `catch (ArgumentCountError)`, `catch (TypeError)`, `catch (Error)`, and `catch (Throwable)` — the first matching clause in source order wins. `Error` and `Exception` remain disjoint branches of `Throwable`: neither catches the other.
- `intdiv($a, 0)` raises a catchable `DivisionByZeroError` with PHP's `Division by zero` message. `intdiv(PHP_INT_MIN, -1)` raises the parent `ArithmeticError` with `Division of PHP_INT_MIN by -1 is not an integer`, matching PHP.

Divergences in the `Error` hierarchy:

- **Static rejection instead of a runtime `ArgumentCountError`/`TypeError`.** elephc is an AOT compiler: a call whose arity or argument types are provably wrong is a COMPILE error, not a runtime throw. `opcache_reset(1)` fails the build with `Function 'opcache_reset' expects 0 arguments, got 1`, where PHP 8.5 would compile it and throw `ArgumentCountError: opcache_reset() expects exactly 0 arguments, 1 given` when the line executes. The `catch (\ArgumentCountError $e)` clause itself compiles fine — only the statically provable error is reported earlier and more loudly. Wrap a genuinely dynamic call if you need the runtime behavior.
- `AssertionError` can be thrown, caught, and inspected from userland, but nothing in elephc raises it: `assert()` is not a supported builtin (`Undefined function: assert`), so there is no `zend.assertions=1` path to fail.
- `UnhandledMatchError` can be thrown and caught from userland, but an unmatched `match` arm is still a fatal terminator rather than a constructed, catchable `UnhandledMatchError` (see the `match` section above).
- `$a / 0` and `$a % 0` do NOT throw. `/` yields `INF` and `%` yields `0`, where PHP 8 raises `DivisionByZeroError` with `Division by zero` / `Modulo by zero`. Use `intdiv()` when you need the catchable error.
- An UNCAUGHT throwable prints `Fatal error: Uncaught <Class>: <message> in <file>:<line>` and exits `255`, matching PHP up to the STACK TRACE, which elephc omits — PHP follows the first line with `Stack trace:`, `#0 {main}` and `  thrown in <file> on line <n>`. The file and line are the CONSTRUCTION site, as in PHP: an exception built on line 2 and thrown on line 5 reports line 2. `getFile()` and `getLine()` return the same pair; `getTrace()` and `getTraceAsString()` remain an empty array and an empty string, because elephc keeps no call stack.
- A throwable raised by a CODEGEN GUARD (`intdiv()` by zero, the `array_keys()` argument check) names its class and message and exits `255` like any other, but carries NO ` in <file>:<line>` suffix: it is synthesized by the compiler rather than by a user `new`, so there is no construction site to report. `getLine()` returns `0` for these. PHP does report the operation's own line here.
- The file reported for an uncaught throwable is the COMPILED SCRIPT's path. EIR spans carry a line and column but no filename, so code merged in from an `include` reports the including script's path rather than its own.
- Engine-only `Error` subclasses are NOT declared: `ParseError`, `CompileError`, `DateError`/`DateObjectError`/`DateRangeError`, `Random\RandomError`/`Random\BrokenRandomEngineError`, `Uri\UriError`, and `FFI\Exception`. They exist in PHP only to report failures elephc either resolves at compile time (a parse error) or does not implement, so declaring them would add a name that nothing can ever raise.
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
