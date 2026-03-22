# elephc Language Reference

This document describes the PHP subset supported by elephc. Every program listed here is valid PHP and produces identical output when run with `php`.

## Data Types

| Type | Supported | Notes |
|---|---|---|
| `int` | Yes | 64-bit signed integer |
| `string` | Yes | Pointer + length pair, double and single quoted |
| `null` | Yes | Sentinel value, coerces to `0`/`""` in operations |
| `bool` | Partial | `true`/`false` exist but are treated as `int(1)`/`int(0)`. `echo false` prints `0` instead of nothing. |
| `float` | No | Not yet supported. Division is integer-only. |
| `array` | Partial | Indexed arrays only. No associative arrays. |
| `object` | No | Not planned (no OOP support). |
| `resource` | No | Not planned. |

### Null behavior

```php
<?php
$x = null;
echo $x;              // prints nothing
echo is_null($x);     // prints 1
echo $x + 5;          // prints 5 (null → 0)
echo $x . "hello";    // prints "hello" (null → "")
$x = 42;              // reassignment from null works
```

### Known incompatibilities with PHP

- `echo false` prints `"0"` in elephc, nothing in PHP. Will be fixed when Bool type is added.
- `echo true` prints `"1"` in both (correct).
- Division is integer-only: `10 / 3` returns `3`, not `3.333...`. Will be fixed when Float type is added.
- `$argv[0]` returns the compiled binary path, not the `.php` file path.

## Operators

### Arithmetic

| Operator | Example | Notes |
|---|---|---|
| `+` | `$a + $b` | Addition |
| `-` | `$a - $b` | Subtraction |
| `*` | `$a * $b` | Multiplication |
| `/` | `$a / $b` | Division (integer-only, no float) |
| `%` | `$a % $b` | Modulo |
| `-$x` | `-$x` | Unary negation |

### Comparison

| Operator | Example | Notes |
|---|---|---|
| `==` | `$a == $b` | Loose equality (integers only) |
| `!=` | `$a != $b` | Inequality |
| `<` | `$a < $b` | Less than |
| `>` | `$a > $b` | Greater than |
| `<=` | `$a <= $b` | Less than or equal |
| `>=` | `$a >= $b` | Greater than or equal |

**Not supported yet:** `===`, `!==` (strict comparison), `<=>` (spaceship).

### Logical

| Operator | Example | Notes |
|---|---|---|
| `&&` | `$a && $b` | AND with short-circuit |
| `\|\|` | `$a \|\| $b` | OR with short-circuit |
| `!` | `!$a` | NOT |

Short-circuit evaluation: if `$a` is false in `$a && $b`, `$b` is not evaluated.

**Not supported yet:** `and`, `or`, `xor` (word-form logical operators).

### String

| Operator | Example | Notes |
|---|---|---|
| `.` | `"a" . "b"` | Concatenation |
| `.` | `"val=" . 42` | Auto-coerces int to string |

### Assignment

| Operator | Example | Equivalent |
|---|---|---|
| `=` | `$x = 5` | Simple assignment |
| `+=` | `$x += 5` | `$x = $x + 5` |
| `-=` | `$x -= 5` | `$x = $x - 5` |
| `*=` | `$x *= 5` | `$x = $x * 5` |
| `/=` | `$x /= 5` | `$x = $x / 5` |
| `%=` | `$x %= 5` | `$x = $x % 5` |
| `.=` | `$s .= "x"` | `$s = $s . "x"` |

**Not supported yet:** `**=`, `&=`, `|=`, `^=`, `<<=`, `>>=`, `??=`.

### Increment / Decrement

| Operator | Example | Returns |
|---|---|---|
| `++$i` | Pre-increment | New value |
| `$i++` | Post-increment | Old value |
| `--$i` | Pre-decrement | New value |
| `$i--` | Post-decrement | Old value |

### Ternary

```php
$max = $a > $b ? $a : $b;
```

**Not supported yet:** `??` (null coalescing), `?:` (short ternary).

## Control Structures

### if / elseif / else

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

### while

```php
<?php
$i = 0;
while ($i < 10) {
    echo $i;
    $i++;
}
```

### do...while

```php
<?php
$i = 0;
do {
    $i++;
} while ($i < 10);
```

### for

```php
<?php
for ($i = 0; $i < 10; $i++) {
    echo $i;
}
```

### foreach

```php
<?php
$arr = [1, 2, 3];
foreach ($arr as $value) {
    echo $value . "\n";
}
```

**Not supported yet:** `foreach ($arr as $key => $value)`.

### break / continue

```php
<?php
for ($i = 0; $i < 100; $i++) {
    if ($i == 5) { break; }
    if ($i % 2 == 0) { continue; }
    echo $i . " ";
}
// Output: 1 3
```

**Not supported:** `break 2;` / `continue 2;` (multi-level).

### switch / match

**Not supported yet.** Use `if`/`elseif`/`else` instead.

## Functions

### Declaration and calls

```php
<?php
function add($a, $b) {
    return $a + $b;
}

echo add(3, 4); // 7
```

### Recursion

```php
<?php
function factorial($n) {
    if ($n <= 1) { return 1; }
    return $n * factorial($n - 1);
}
echo factorial(10); // 3628800
```

### Void functions

```php
<?php
function greet($name) {
    echo "Hello, " . $name . "\n";
    return;
}
greet("World");
```

### Local scope

Variables inside a function are separate from the caller:

```php
<?php
$x = 1;
function get_x() {
    $x = 99;
    return $x;
}
echo $x;       // 1
echo get_x();  // 99
```

### Limitations

- No default parameter values (`function foo($x = 10)` — not supported)
- No pass by reference (`function foo(&$x)` — not supported)
- No variadic functions (`function foo(...$args)` — not supported)
- No anonymous functions / closures
- No `global` keyword

## Strings

### Double-quoted strings

Support escape sequences:

```php
<?php
echo "Hello\n";      // newline
echo "Tab\there";    // tab
echo "Quote: \"";    // escaped quote
echo "Backslash: \\"; // backslash
```

### Single-quoted strings

No escape sequences except `\\` and `\'`:

```php
<?php
echo 'Hello\n';      // prints: Hello\n (literal backslash-n)
echo 'It\'s here';   // prints: It's here
```

### String interpolation

**Not supported yet.** Use concatenation:

```php
<?php
$name = "World";
echo "Hello, " . $name . "\n";  // works
// echo "Hello, $name\n";       // NOT supported yet
```

## Arrays

### Indexed arrays

```php
<?php
$arr = [10, 20, 30];
echo $arr[0];          // 10
echo count($arr);      // 3

$arr[1] = 99;          // modify element
$arr[] = 40;           // push element

foreach ($arr as $v) {
    echo $v . " ";
}
```

### String arrays

```php
<?php
$names = ["Alice", "Bob", "Charlie"];
foreach ($names as $name) {
    echo "Hello, " . $name . "\n";
}
```

### Limitations

- No associative arrays (`["key" => "value"]` — not supported)
- No `foreach ($arr as $key => $value)` (key binding)
- No multi-dimensional arrays
- No array union operator (`+`)
- Arrays are homogeneous: all elements must be the same type

## Built-in Functions

### String functions

| Function | Signature | Description |
|---|---|---|
| `strlen()` | `strlen($str): int` | Returns string length |
| `intval()` | `intval($val): int` | Converts to integer |

### Array functions

| Function | Signature | Description |
|---|---|---|
| `count()` | `count($arr): int` | Number of elements |
| `array_push()` | `array_push($arr, $val): void` | Add element to end |
| `array_pop()` | `array_pop($arr): mixed` | Remove and return last element |
| `in_array()` | `in_array($needle, $arr): int` | Search for value (returns 0/1) |
| `array_keys()` | `array_keys($arr): array` | Returns [0, 1, 2, ...] |
| `array_values()` | `array_values($arr): array` | Returns copy of values |
| `sort()` | `sort($arr): void` | Sort ascending (in-place) |
| `rsort()` | `rsort($arr): void` | Sort descending (in-place) |
| `isset()` | `isset($var): int` | Check if variable is defined (always 1) |

### Type functions

| Function | Signature | Description |
|---|---|---|
| `is_null()` | `is_null($val): int` | Returns 1 if null, 0 otherwise |

### System functions

| Function | Signature | Description |
|---|---|---|
| `exit()` | `exit($code = 0): void` | Terminate program |
| `die()` | `die(): void` | Alias for `exit(0)` |

## Superglobals

| Variable | Type | Description |
|---|---|---|
| `$argc` | `int` | Number of command-line arguments |
| `$argv` | `array(string)` | Command-line argument values |

```php
<?php
echo "Program: " . $argv[0] . "\n";
echo "Args: " . $argc . "\n";
for ($i = 1; $i < $argc; $i++) {
    echo "  " . $argv[$i] . "\n";
}
```

## Comments

```php
<?php
// Single-line comment

/* Multi-line
   comment */

echo "code"; // inline comment
/* a *//* b */ // consecutive comments work
```

## What elephc cannot do (by design)

- No classes, objects, interfaces, traits, enums
- No exceptions (`try`/`catch`/`throw`)
- No `eval()`
- No `include`/`require` (yet)
- No namespaces
- No generators/yield
- No fibers
- No attributes
- No named arguments
