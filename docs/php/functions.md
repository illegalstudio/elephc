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

## Parameter and return type hints

```php
<?php
function repeat(string $label, int $count): string {
    return $label . $count;
}
```

- Supported types: `int`, `float`, `bool`, `string`, `array`, `callable`, `ptr`, class/interface/enum names
- `mixed`, union, and nullable type hints supported
- `void` is valid only as a return type
- Typed parameters can use default values
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
$double = function($x) {
    return $x * 2;
};
echo $double(5); // 10
```

Closures can capture with `use`:

```php
<?php
$factor = 3;
$multiply = function($x) use ($factor) {
    return $x * $factor;
};
echo $multiply(5); // 15
```

**Limitation:** Closures with `use` captures cannot be passed to `array_map`, `array_filter`, etc.

## Arrow functions

```php
<?php
$double = fn($x) => $x * 2;
echo $double(5); // 10

$nums = [1, 2, 3, 4];
$squared = array_map(fn($n) => $n * $n, $nums);
```

## First-class callable syntax

```php
<?php
$triple = triple(...);
$double = MathBox::double(...);
```

Supported: `functionName(...)`, `ClassName::method(...)`, `self::method(...)`, `parent::method(...)`.
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

`print` works as an alias for `echo`. Always returns 1 but cannot be used as expression.
