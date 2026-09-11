---
title: "Functions"
description: "Function declarations, closures, arrow functions, variadic parameters, and more."
sidebar:
  order: 4
---

## Declaration and calls

```php
<?php
function add($a, $b) {
    return $a + $b;
}
echo add(3, 4); // 7
```

Function lookup is case-insensitive like PHP. The declaration keeps its original
name, but calls such as `ADD(3, 4)`, `Add(3, 4)`, and `add(3, 4)` resolve to the
same function. Built-in function names follow the same rule.

## Parameter and return type hints

```php
<?php
function repeat(string $label, int $count): string {
    return $label . $count;
}
```

- Supported types: `int`, `float`, `bool`, `string`, `array`, `mixed`, `iterable`, `callable`, `ptr`, `buffer<T>`, class/interface/enum names, unions, and nullable forms
- `void` is valid only as a return type
- `never` is valid only as a return type and must not return normally
- Typed parameters can use default values
- Function, method, constructor, closure, and arrow-function parameter hints are checked
- Function, method, closure, and arrow-function return type hints are checked
- Arguments are bound to declared scalar parameters using PHP's default (coercive) rules where elephc can reproduce them exactly — `takesString(42)` passes `"42"`, `takesInt(5.0)` passes `5`. Conversions PHP decides at run time with a `Deprecated:` notice or a `TypeError` are compile errors instead. A file that opens with `declare(strict_types=1)` switches to PHP's strict binding, where only an exact type match and the `int`→`float` widening are accepted; see [Types → Parameter type coercion](./types.md#parameter-type-coercion) and [Types → Strict types](./types.md#strict-types) for the full tables
- A `callable` parameter accepts a compile-time-constant callable string (`"strtoupper"`, `"Formatter::wrap"`) as well as closures and first-class callables; see [Types → Callable strings](./types.md#callable-strings)
- Variadic parameters may carry a type hint (`function f(int ...$xs)`), including on methods, closures, and arrow functions; every argument collected into the variadic is checked against the declared element type, just like a regular typed parameter. An untyped variadic accepts heterogeneous arguments.
- Non-`void` declared return types must return a value on every reachable path; `throw`, `exit()`/`die()`, and infinite loops count as non-returning paths
- Bare `return;` is valid only for `void` returns; use `return null;` for nullable return types
- Named arguments are supported for known-signature calls: user-defined functions, methods, closures, built-ins, and extern functions
- Callable variables and `callable` parameters whose concrete target is known only through a runtime descriptor can also use named arguments, named-after-spread calls, and positional prefixes before indexed spreads; descriptor metadata applies parameter names, defaults, variadics, and by-reference flags at invocation time
- Argument expressions are evaluated in PHP source order, then codegen normalizes the resulting values into ABI parameter order
- Named arguments can follow spread arguments, as in `foo(...$args, suffix: "!")`; positional arguments cannot follow either named arguments or spread arguments
- Argument unpacking cannot follow a named argument: `foo(c: 9, ...$args)` is a compile-time error ("cannot use argument unpacking after named arguments"), matching PHP's fatal. The rule is syntactic, so it applies whatever the unpacked array contains — including a static string-keyed literal such as `foo(c: 9, ...["a" => 1])` — and on every call surface, including calls whose target is only known at run time. Back-to-back spreads (`foo(...$a, ...$b)`) and a string-keyed spread on its own (`foo(...["a" => 1])`) stay legal.
- Associative-array unpacking maps string keys to named arguments (`foo(...["name" => "Ada"])`) and keeps numeric keys positional. Variable associative-array spreads can satisfy any parameter by string key, including parameters after explicit named arguments. Duplicate static string keys use PHP's last-wins behavior before argument planning.
- A positional spread into a variadic function fills regular parameters first; only excess spread elements are collected into the variadic parameter. If a spread is too short to fill required parameters, the call fails instead of reading beyond the array payload.
- User-defined variadic functions collect unknown named arguments into the variadic parameter using string keys
- Built-in variadic functions reject unknown named arguments, matching PHP's internal-function behavior

## Recursion

```php
<?php
function factorial($n) {
    if ($n <= 1) { return 1; }
    return $n * factorial($n - 1);
}
echo factorial(10); // 3628800
```

Recursion depth is bounded by the real call stack, and running off the end is
reported instead of crashing. Every compiled function checks the stack pointer
against the measured stack floor on entry; when it is exhausted the program
writes

```
Fatal error: Maximum call stack size reached. Infinite recursion?
```

to stderr and exits with status 255 — the same class of controlled diagnostic
PHP 8.3+ produces for runaway recursion, and the same exit status PHP uses for
an uncaught fatal error.

The floor comes from `getrlimit(RLIMIT_STACK)` minus a small reserve, so the
usable depth follows the process stack limit: roughly 50 000 frames of a small
function on a default 8 MiB stack. Function bodies that run on a coroutine
stack — generator bodies and `Fiber` callables — get a floor derived from that
coroutine's own 256 KiB stack instead, which is roughly 1 400 frames of the same
function. Deepen `ulimit -s` if a legitimately deep algorithm needs more room on
the main stack.

## Default parameter values

```php
<?php
function greet($name = "world") {
    echo "Hello " . $name . "\n";
}
greet();        // Hello world
greet("PHP");   // Hello PHP
```

Parameters with defaults must come after required parameters.

## Local scope

Variables inside a function are separate from the caller.

## Anonymous functions (closures)

```php
<?php
$double = function(int $x): int {
    return $x * 2;
};
echo $double(5); // 10
```

Closures can capture with `use`:

```php
<?php
$factor = 3;
$multiply = function(int $x) use ($factor): int {
    return $x * $factor;
};
echo $multiply(5); // 15
```

Use `&` in the capture list when the closure must share and mutate the outer
variable, including recursive anonymous functions:

```php
<?php
$factorial = null;
$factorial = function($n) use (&$factorial) {
    return $n <= 1 ? 1 : $n * $factorial($n - 1);
};
echo $factorial(5); // 120
```

Captured closures can also be used as callback values:

```php
<?php
$factor = 3;
$values = array_map(function(int $x) use ($factor): int {
    return $x * $factor;
}, [1, 2, 3]);
echo $values[2]; // 9
```

A closure parameter typed `mixed` (or left untyped, which is `mixed`) accepts any value,
and a closure whose body is `return $param;` infers a `mixed` return type. This lets
`array_map()` and the other callback built-ins run over heterogeneous arrays whose elements
have different types:

```php
<?php
$mixed = [1, "two", 3.5, true];
$same = array_map(function(mixed $x) { return $x; }, $mixed);
echo $same[1];                 // two — the string element is preserved, not coerced
$types = array_map(fn($x) => gettype($x), $mixed);
echo $types[0] . " " . $types[2]; // integer double
```

## Binding `$this` in closures

A non-static closure or arrow function defined inside an instance method
automatically binds `$this` — no `use ($this)` is needed (and, as in PHP,
`use ($this)` is not allowed). The closure sees the live object, so reads,
writes, and method calls through `$this` all work and persist on the instance:

```php
<?php
class Counter {
    private int $count = 0;

    public function incrementer(): callable {
        return function (): int {
            $this->count += 1;       // mutates the live object
            return $this->count;
        };
    }

    public function labelled(string $suffix): callable {
        return fn (): string => $this->count . $suffix;  // arrow binds $this too
    }
}

$c = new Counter();
$next = $c->incrementer();
echo $next(), $next();           // 12
echo ($c->labelled("!"))();      // 2!
```

`$this` also flows into nested closures: an inner closure defined inside an
outer one captures `$this` transitively from the enclosing scope.

### Rebinding `$this` with `Closure::bind` / `bindTo`

A closure's bound `$this` can be swapped for another object, producing a new
closure. Both the instance method `$closure->bindTo($newThis)` and the static
`Closure::bind($closure, $newThis)` are supported; the original closure is left
unchanged:

```php
<?php
class Box {
    public int $value;
    public function __construct(int $value) { $this->value = $value; }
    public function reader(): callable {
        return function (): int { return $this->value; };
    }
}

$a = new Box(7);
$b = new Box(99);
$read = $a->reader();
echo $read();              // 7

$rebound = $read->bindTo($b);
echo $rebound();           // 99  (rebound to $b)

$static = Closure::bind($read, $b);
echo $static();            // 99  (static spelling)

echo $read();              // 7   (original is unchanged)
```

An optional third `$scope` argument is accepted for source compatibility and
ignored (member visibility is resolved at compile time).

`$closure->call($newThis, ...$args)` binds `$this` and invokes the closure in a
single step, returning its result:

```php
<?php
$add = $a->reader();              // reusing Box from above (returns $this->value)
echo $add->call($b);              // 99 — bound to $b for this one call
```

A closure defined outside any class may also reference `$this` and be bound
later — the canonical "scope-stealing" accessor:

```php
<?php
class Account {
    private int $balance = 250;
}

$peek = function() { return $this->balance; };
$read = Closure::bind($peek, new Account(), Account::class);
echo $read();   // 250 — bound access reaches the private property
```

Rebinding supports closures that capture `$this` and nothing else (the typical
accessor closure, whether created inside a method or standalone). Binding a
closure that also has `use(...)` captures aborts with a fatal error rather than
producing an incorrectly bound closure.

## Static closures

A closure prefixed with `static` does not capture `$this` from its enclosing
scope. This matches PHP's `static function () {}` and `static fn () => ...` —
useful when a closure is meant to be unbound (often paired with
`Closure::bind(..., null, ...)`):

```php
<?php
$add = static function ($a, $b) {
    return $a + $b;
};
echo $add(3, 4);                     // 7

$double = static fn ($x) => $x * 2;
echo $double(5);                     // 10
```

Inside a static closure, referencing `$this` is a compile-time error:

```php
<?php
class C {
    public int $count = 5;
    public function bad() {
        // Error: Cannot use $this inside a static closure
        return static function () { return $this->count; };
    }
}
```

## Arrow functions

```php
<?php
$double = fn(int $x): int => $x * 2;
echo $double(5); // 10

$nums = [1, 2, 3, 4];
$squared = array_map(fn(int $n): int => $n * $n, $nums);
```

## First-class callable syntax

```php
<?php
$triple = triple(...);
$double = MathBox::double(...);
```

Supported: user-defined function names, extern function names, `ClassName::method(...)`, `self::method(...)`, `parent::method(...)`, and registered builtin wrappers. Builtin wrapper coverage includes common string transforms and searches (`strlen(...)`, `trim(...)`, `substr(...)`, `str_contains(...)`), casts and type checks (`intval(...)`, `floatval(...)`, `gettype(...)`, `is_int(...)`), math helpers (`abs(...)`, `sqrt(...)`, `round(...)`), JSON helpers, and array helpers including `count(...)`, `array_sum(...)`, `array_product(...)`, and by-reference mutators such as `sort(...)`.
Also supported: `static::method(...)` inside class methods, preserving late static binding for direct callable calls, and `$obj->method(...)` / `$this->method(...)` with either a local receiver variable or a non-local receiver expression such as `(new Greeter("Hi "))->greet(...)`.

```php
<?php
class Greeter {
    public function hello($name) {
        return "Hello " . $name;
    }
}

$greeter = new Greeter();
$hello = $greeter->hello(...);
echo $hello("Ada"); // Hello Ada
```

Captured first-class callable targets (`static::method(...)` and `$obj->method(...)`) can be called directly through a local callable variable or as an immediate callable expression such as `($obj->method(...))("Ada")`. Branch-shaped immediate calls and equivalent `call_user_func()` / `call_user_func_array()` calls that select captured callable descriptors at runtime, such as `($ok ? $a->method(...) : $b->method(...))($value)`, `($ok ? $a->method(...) : $b->method(...))(...$args, suffix: "!")`, `call_user_func($ok ? $a->method(...) : $b->method(...), $value)`, or `call_user_func_array($ok ? $a->method(...) : $b->method(...), [$value])`, route through descriptor invokers for positional arguments, named arguments, spread prefixes, defaults, and variadics. Method first-class callable variables also invoke through the stored descriptor environment, so `$fn = $obj->method(...); $obj = $other; $fn()` still uses the receiver captured when `$fn` was created while descriptor metadata applies names, defaults, variadics, and by-reference flags. A callable variable or array element whose descriptor was selected earlier at runtime also invokes through the descriptor metadata, including by-reference parameter decisions that are only known from the stored descriptor. `callable` parameters with no local signature metadata follow the same descriptor path for named arguments and positional prefixes before indexed spreads, including source-variable mutation for runtime by-reference parameters. Direct captured callable values can also be passed to callback paths that forward captured callable environments, including `array_map()`, `array_filter()`, `array_reduce()`, `array_walk()`, `usort()`, `uksort()`, `uasort()`, `preg_replace_callback()`, `call_user_func()`, and `call_user_func_array()`. When a captured callable is stored in a local variable or received through a `callable` parameter, these callback runtimes retain the descriptor itself rather than rebuilding captures from current source locals. Branch-shaped runtime selection of captured callable descriptors is supported for `array_map()`, `array_filter()`, `array_reduce()`, `array_walk()`, `usort()`, `uksort()`, `uasort()`, `preg_replace_callback()`, `iterator_apply()`, `CallbackFilterIterator`, and `RecursiveCallbackFilterIterator`. Callable descriptors carry signature defaults, by-reference flags, variadic metadata, receiver/capture environments, and the invocation shape for function, builtin, extern, closure, first-class, static-method, instance-method, callable-array, and invokable-object forms. For by-reference callback parameters, `call_user_func()` preserves source-variable mutation even when the visible callback signature is known only through the runtime descriptor. `call_user_func_array()` passes original variable slots from literal argument arrays such as `call_user_func_array($cb, [$value])`; dynamic argument arrays are accepted through temporary reference cells, so callback writes do not mutate the source array or source variable. PHP disallows nullsafe first-class callable syntax (`$obj?->method(...)`), and elephc reports the same error.

`call_user_func()` and `call_user_func_array()` also accept PHP callable arrays (`[$object, "method"]`, `[ClassName::class, "method"]`) and invokable objects. Dynamic string callback dispatch resolves user functions, declared extern functions, supported builtin wrappers such as `STRLEN`, and public static method strings such as `"Formatter::wrap"`; the matched descriptor's generated invoker receives a boxed argument container, branches on whether it holds an indexed array or associative hash, applies the resolved descriptor signature, and returns a boxed `mixed`. String variables can also be invoked directly as PHP variable functions, for example `$callback = "STRLEN"; echo $callback("hello");`, and they use the same descriptor invoker path for names, defaults, variadics, and by-reference parameter metadata. Callable-array variables and literals can also be invoked directly, for example `$callback = [$object, "wrap"]; echo $callback(value: "ok");` or `([$object, "wrap"])(value: "ok")`, and direct instance-method callable arrays read the receiver stored in the callable array before invoking the descriptor. Objects with public `__invoke()` can also be called directly through descriptor metadata, so `$runner(suffix: "?")` and `(new Runner())(suffix: "?")` apply defaults, named arguments, variadics, and by-reference flags through the same invoker path. Direct callable-array variables and literals may resolve the method or static receiver from runtime strings, so `$method = "wrap"; $callback = [$object, $method]; $callback(...)`, `([$object, $method])(...)`, `$callback = [$class, $method]; ($callback)(...)`, and `([$class, $method])(...)` select the matching public method descriptor at the call site. Public static-method callable arrays use the same descriptor invoker path for direct variable and literal calls, `call_user_func()`, and `call_user_func_array()`, including associative argument containers and positional prefixes before indexed spreads. Public instance-method callable arrays and invokable objects use descriptor invokers for direct `call_user_func()` calls, including single-spread forwarding such as `call_user_func([$object, "method"], ...$args)` and positional prefixes followed by indexed spreads such as `call_user_func([$object, "method"], "head", ...$args)`. They use the same descriptor path for `call_user_func_array()` calls with literal indexed, literal associative, dynamic indexed, dynamic associative, or runtime-opaque `mixed`/union argument containers. Receiver-bound runtime-opaque containers are unboxed at runtime, dispatch by indexed-array versus associative-hash tag, and prepend the receiver as descriptor slot zero before invoking the descriptor adapter. For `call_user_func_array()`, descriptor invokers operate on a cloned Mixed argument container so the caller's `$args` array keeps its original layout after invocation.

## Global variables

```php
<?php
$x = 10;
function test() {
    global $x;
    echo $x;    // 10
}
```

## Static variables

```php
<?php
function counter() {
    static $n = 0;
    $n++;
    echo $n . "\n";
}
counter(); // 1
counter(); // 2
```

The initializer is optional: `static $x;` declares the variable with an
implicit `null` initial value, exactly like `static $x = null;`.

## Pass by reference

```php
<?php
function increment(&$val) {
    $val++;
}
$x = 5;
increment($x);
echo $x; // 6

$a = 1;
$b =& $a;
$b = 2;
echo $a; // 2
```

## References to array elements

A local can alias an element of an **indexed array** (`$b =& $a[0]`). Writing through
the alias updates the element, and writing the element updates the alias:

```php
<?php
$a = [1, 2, 3];
$b =& $a[0];
$b = 9;
echo $a[0]; // 9
$a[0] = 5;
echo $b;    // 5
```

Current limits (see the incompatibilities list in [types](./types.md)):

- The source must be an indexed array with an integer index. Associative arrays
  (`$b =& $a['key']`) are rejected at compile time.
- Referencing an out-of-range index does **not** create the element the way PHP's
  autovivification does; the alias binds to a null cell instead.
- The array must stay alive (and must not grow) while the alias is used: the alias
  points into the array's storage, and operations that reallocate it (such as
  `$a[] = ...`) detach the alias.

## References to object properties

A local can alias an object property. Writing through the alias updates the property,
and writing the property updates the alias — both names share one storage cell:

```php
<?php
class Counter {
    public int $value = 0;
}

$counter = new Counter();
$ref = &$counter->value;
$ref = 10;
echo $counter->value; // 10
$counter->value = 42;
echo $ref;            // 42
```

This works for array properties too, so the alias can be mutated in place:

```php
<?php
class Bag { public array $items = []; }
$bag = new Bag();
$items = &$bag->items;
$items[] = "apple";
echo implode(", ", $bag->items); // apple
```

Aliasing works for any declared property type, including `string` and `float`. Reassigning
an array alias to a differently-typed literal keeps the property readable, because the new
elements are converted to match the property's element type:

```php
<?php
class Bag { public array $items = []; }
$bag = new Bag();
$items = &$bag->items;
$items = [1, 2, 3];                  // typed literal, boxed to match the property
echo implode(", ", $bag->items);     // 1, 2, 3
```

## Reference returns

A function or method declared with `&` before its name returns a reference to the
returned lvalue rather than a copy. The caller captures it with `$x = &f()`; writing
`$x` then writes through to the original storage:

```php
<?php
class Registry {
    public array $entries = [];
    public function &all() {
        return $this->entries;
    }
}

$registry = new Registry();
$entries = &$registry->all();
$entries[] = "first";
echo implode(", ", $registry->entries); // first

function &slot(Counter $c) {
    return $c->value;
}
```

Closures and arrow functions may also return by reference, including when bound to an
object with `Closure::bind`. The bound closure can be invoked immediately or stored in a
variable and called later:

```php
<?php
class Container { public array $services = []; }
$container = new Container();

// Immediate invocation.
$services = &Closure::bind(fn &() => $this->services, $container, $container)();
$services[] = "logger";          // reaches $container->services
$services = [];                  // clears $container->services through the reference

// Stored and called later.
$accessor = Closure::bind(fn &() => $this->services, $container, $container);
$ref = &$accessor();
$ref[] = "router";               // also reaches $container->services
```

Reference returns are supported for object-property targets of any scalar or container
type: arrays, objects, integers, strings, and floats. A plain local is also supported.
It is promoted to a managed reference cell that keeps the variable's identity and outlives
the call, so the caller's alias and any later write through the same variable agree.

A `finally` clause cannot change which reference a pending `return` hands back. The
reference is captured when the `return` runs, exactly as in PHP, so rebinding the returned
variable inside a `finally` that falls through has no effect on the caller. A second
`return` inside the `finally` does replace it.

The caller's reference is published before any of the caller's own post-call cleanup runs,
so a destructor that throws while the call's argument temporaries, an owning receiver, or
the previous binding of the target are retired cannot lose the returned reference. A
`catch` in the same function still sees the referenced value released exactly once.

### Reference-return forms this compiler does not support yet

The forms below are valid PHP. They are rejected at compile time because this compiler
cannot yet transfer an owning reference cell for them, not because PHP lacks the
semantics:

- Returning an alias of an array element (`$slot = &$numbers[0]; return $slot;`). That
  alias addresses storage inside the array's payload, which owns no reference cell of its
  own, so there is nothing to hand the caller that would keep the element alive.
- Returning a place that is neither a variable nor a property, such as an array element
  read or the result of another call. PHP returns a reference to those places; this
  lowering has no cell to transfer for them.
- Capturing a reference with `$x = &f()` from a call the compiler cannot resolve to a
  by-reference return. A named function, a method, a static method, and a closure whose
  binding is known (including one produced by `Closure::bind`) are all resolved. A call
  made through a runtime-selected callable, such as a variable holding a function-name
  string, is not: that path is lowered through the dynamic descriptor invoker, which hands
  back a copied value rather than the callee's reference cell.

One case cannot be decided at compile time. A function that returns its own by-reference
parameter has no way to know what the caller bound to it, so a caller that passed an alias
of an array element is caught at run time instead: the return raises a catchable `Error`,
`Cannot return a reference to storage that has no independent reference cell`, rather than
handing back an address the array may free.

## Variadic functions

```php
<?php
function sum(...$nums) {
    $total = 0;
    foreach ($nums as $n) {
        $total += $n;
    }
    return $total;
}
echo sum(1, 2, 3); // 6
```

Variadic parameters can appear on functions, methods, closures, and arrow
functions, and may carry a type hint:

```php
<?php
function join_with(string $sep, string ...$parts): string {
    return implode($sep, $parts);
}
echo join_with("-", "a", "b", "c"); // a-b-c
```

A type hint on the variadic constrains its elements: `function sum(int ...$nums)`
collects integers into `$nums`, and every argument passed to the variadic is checked
against the declared element type, so passing an argument of the wrong type is
rejected. An untyped variadic (`...$nums`) accepts heterogeneous arguments.

## Argument introspection

`func_num_args()`, `func_get_args()` and `func_get_arg($position)` read the arguments the
current call actually received, including the surplus positional arguments PHP allows past
a function's declared parameter list:

```php
<?php
function log_all() {
    echo func_num_args(), ": ";
    foreach (func_get_args() as $arg) {
        echo $arg, " ";
    }
}
log_all("a", "b", "c"); // 3: a b c

function first_extra($label) {
    return func_get_arg(1);
}
echo first_extra("label", "extra"); // extra
```

They work in functions, methods (instance and static), closures and arrow functions, and
report the *current* values of the declared parameters, so a parameter the body reassigned
— or wrote through by reference — is reflected in `func_get_args()`, exactly as in PHP. An
out-of-range or negative position throws `ValueError` with PHP's message.

elephc implements these constructs by giving the function a hidden variadic parameter that
collects the surplus arguments, which constrains where they can be used. They are rejected,
with a diagnostic naming the reason, when:

- the call is outside any function (PHP raises the same "must be called from a function
  context" error at runtime);
- the function declares a parameter with a default value — the hidden variadic cannot tell
  a passed argument from a defaulted one;
- the function already declares its own variadic parameter — read that parameter instead;
- the method overrides a parent method or implements an interface method — the inherited
  signature has no slot for the collected arguments;
- the call is dynamic (`func_num_args(...)`, `$f = "func_num_args"; $f()`), which PHP also
  rejects with "Cannot call func_num_args() dynamically".

Surplus *positional* arguments are only accepted by functions that use one of these three
constructs; every other function keeps elephc's compile-time arity check. Surplus *named*
arguments stay rejected either way, matching PHP.

Because the three constructs are rewritten by the compiler rather than dispatched as
builtin calls, `function_exists()` reports `false` for them, where PHP reports `true`.

## Spread operator

```php
<?php
$args = [10, 20, 30];
echo sum(...$args); // 60

$a = [1, 2];
$b = [3, 4];
$c = [...$a, ...$b]; // [1, 2, 3, 4]
```

Call unpacking follows PHP's parameter mapping. Spread values fill regular
positional parameters before any variadic tail is built, and associative-array
spreads treat string keys as named arguments:

```php
<?php
function show($a, $b = 99, ...$rest) {
    echo $a . ":" . $b . ":" . count($rest);
}

show(...[10]);                    // 10:99:0
show(...[10, 20, 30]);            // 10:20:1
show(...[10, "b" => 20]);         // 10:20:0

$args = ["b" => 20, "a" => 10];
show(...$args);                   // 10:20:0

$args = ["b" => 20];
show(...$args, a: 10);            // 10:20:0
```

## echo

`echo` is a PHP language construct statement. It writes each operand to stdout
using PHP scalar output rules and accepts PHP-compatible comma-separated
operands:

```php
<?php
echo "Hello", ", ", "World!\n";
```

## print

`print` is a PHP language construct expression. It writes its operand to stdout
using the same scalar output rules as `echo`, then returns `1`.

```php
<?php
$ok = print "ready\n";
echo $ok;             // 1

echo print "nested";  // prints "nested1"
```

As in PHP, `print` can also stand alone as a statement:

```php
<?php
print "hello\n";
```
