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
- Variadic parameters are supported only without a parameter type hint for now; `function f(int ...$xs)`, typed variadic methods, typed variadic closures, and typed variadic arrow functions are rejected.
- Non-`void` declared return types must return a value on every reachable path; `throw`, `exit()`/`die()`, and infinite loops count as non-returning paths
- Bare `return;` is valid only for `void` returns; use `return null;` for nullable return types
- Named arguments are supported for known-signature calls: user-defined functions, methods, closures, built-ins, and extern functions
- Callable variables and `callable` parameters whose concrete target is known only through a runtime descriptor can also use named arguments, named-after-spread calls, and positional prefixes before indexed spreads; descriptor metadata applies parameter names, defaults, variadics, and by-reference flags at invocation time
- Argument expressions are evaluated in PHP source order, then codegen normalizes the resulting values into ABI parameter order
- Named arguments can follow spread arguments, as in `foo(...$args, suffix: "!")`; positional arguments cannot follow either named arguments or spread arguments
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

## Pass by reference

```php
<?php
function increment(&$val) {
    $val++;
}
$x = 5;
increment($x);
echo $x; // 6
```

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
functions, but the variadic parameter itself cannot carry a type hint yet. Use
`function sum(...$nums)` instead of `function sum(int ...$nums)`.

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
