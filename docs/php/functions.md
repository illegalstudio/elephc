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
- Non-`void` declared return types must return a value on every reachable path; `throw`, `exit()`/`die()`, and infinite loops count as non-returning paths
- Bare `return;` is valid only for `void` returns; use `return null;` for nullable return types
- Named arguments are supported for known-signature calls: user-defined functions, methods, closures, built-ins, and extern functions
- Argument expressions are evaluated in PHP source order, then codegen normalizes the resulting values into ABI parameter order
- Named arguments can follow spread arguments, as in `foo(...$args, suffix: "!")`; positional arguments cannot follow either named arguments or spread arguments
- Static associative-array unpacking maps string keys to named arguments (`foo(...["name" => "Ada"])`) and keeps numeric keys positional. Duplicate static string keys use PHP's last-wins behavior before argument planning.
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

Supported: user-defined function names, extern function names, `ClassName::method(...)`, `self::method(...)`, `parent::method(...)`, and the registered builtin wrappers `strlen(...)`, `count(...)`, `buffer_len(...)`, `intval(...)`, `strtolower(...)`, `strtoupper(...)`, `ucfirst(...)`, `lcfirst(...)`, `strrev(...)`, `addslashes(...)`, `stripslashes(...)`, `nl2br(...)`, `bin2hex(...)`, `hex2bin(...)`, `htmlspecialchars(...)`, `htmlentities(...)`, `html_entity_decode(...)`, `urlencode(...)`, `urldecode(...)`, `rawurlencode(...)`, `rawurldecode(...)`, `base64_encode(...)`, `base64_decode(...)`, `array_sum(...)`, and `array_product(...)`.
Not supported: `static::method(...)`, `$obj->method(...)`.

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
positional parameters before any variadic tail is built, and static
associative-array spreads treat string keys as named arguments:

```php
<?php
function show($a, $b = 99, ...$rest) {
    echo $a . ":" . $b . ":" . count($rest);
}

show(...[10]);                    // 10:99:0
show(...[10, 20, 30]);            // 10:20:1
show(...[10, "b" => 20]);         // 10:20:0
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
