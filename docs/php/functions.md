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
- Named arguments supported for user-defined functions (reordered at compile time)
- Named arguments not supported for built-in functions, extern functions, or calls mixed with spread arguments

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

**Limitation:** Closures with `use` captures cannot be passed to `array_map`, `array_filter`, etc.

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

Supported: user-defined function names, extern function names, the registered builtin wrappers `strlen(...)`, `count(...)`, and `buffer_len(...)`, plus `ClassName::method(...)`, `self::method(...)`, and `parent::method(...)`.
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
