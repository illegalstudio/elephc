# elephc Language Reference

This document describes the PHP subset supported by elephc. The language aims to stay PHP-compatible, but elephc also exposes compiler-specific pointer features such as `ptr()` and `ptr_cast<T>()`, plus hot-path data extensions such as `packed class` and `buffer<T>`, that are intentionally outside standard PHP syntax.

## Data Types

| Type | Supported | Notes |
|---|---|---|
| `int` | Yes | 64-bit signed integer |
| `string` | Yes | Pointer + length pair, double and single quoted |
| `null` | Yes | Sentinel value, coerces to `0`/`""` in operations |
| `bool` | Yes | `true`/`false` as distinct type. `echo false` prints nothing, `echo true` prints `1`. Coerces to 0/1 in arithmetic. |
| `float` | Yes | 64-bit double-precision. Literals: `3.14`, `.5`, `1.5e3`, `1.0e-5`. Constants: `INF`, `NAN`. |
| `array` | Yes | Indexed (`[1, 2, 3]`) and associative (`["key" => "value"]`). Arrays use copy-on-write semantics: assignments and by-value calls share storage until the first write. |
| `mixed` | Internal | Static helper type used when an associative array stores heterogeneous values. Runtime values are boxed with a per-entry tag, but PHP source code does not spell this type explicitly. |
| `object` | Yes | Class instances. Heap-allocated, fixed-layout. `new ClassName(...)` |
| `pointer` | Yes | 64-bit memory address. `ptr($var)`, `ptr_null()`. Echo prints `0x...` hex. |
| `buffer<T>` | Extension | Contiguous heap buffer for POD scalars, pointers, or packed classes. Allocate with `buffer_new<T>(len)` and query with `buffer_len($buf)`. |
| `packed class` | Extension | Nominal POD record type with fixed compile-time field offsets. Intended for hot-path storage and typed pointer access. |
| `resource` | No | File handles are currently modeled as integer file descriptors (`int`), not as a separate runtime resource type. |

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

- `$argv[0]` returns the compiled binary path, not the `.php` file path.
- `strpos()` returns `-1` when the needle is not found, not `false`. Use `strpos($s, "x") !== -1` or `strpos($s, "x") >= 0` instead of `strpos($s, "x") !== false`.
- `array_search()` returns `-1` when the value is not found, not `false`. Use `array_search($v, $arr) !== -1` or `array_search($v, $arr) >= 0` instead of `array_search($v, $arr) !== false`.
- Integer overflow wraps instead of promoting to float. In PHP, `PHP_INT_MAX + 1` returns a float (`9.2233720368548E+18`); in elephc it wraps to `-9223372036854775808` (native 64-bit signed integer behavior). This is by design — runtime overflow detection would require checking the CPU overflow flag after every arithmetic operation, which is incompatible with the ahead-of-time compilation model.
- Loose comparison (`==`) between different types coerces both sides to integer. PHP has more nuanced type juggling rules (e.g., numeric strings compared as numbers). elephc simplifies this: strings are parsed as integers (empty string and non-numeric strings become `0`).

## Compiler-specific extensions

elephc keeps ordinary supported syntax PHP-compatible, but it also exposes a few build-time or native-interop extensions that are not valid PHP source.

### `ifdef`

`ifdef` is a build-time conditional statement controlled by compiler flags:

```php
<?php
ifdef DEBUG {
    echo "debug\n";
} else {
    echo "release\n";
}
```

Compile with:

```bash
elephc --define DEBUG app.php
```

Rules:
- Syntax is `ifdef SYMBOL { ... }` with optional `else { ... }`
- Bodies must be braced
- Symbols come only from compiler `--define` flags, not from PHP `const` or `define()`
- Inactive branches are removed before `include` / `require` resolution, type checking, and code generation
- `ifdef` is an elephc extension and is not PHP-compatible syntax

The existing pointer helpers such as `ptr()` and `ptr_cast<T>()` remain elephc-specific as well.

### Hot-path data: `packed class` and `buffer<T>`

These features are intended for performance-sensitive code that needs predictable layouts and contiguous storage:

```php
<?php
packed class Vec2 {
    public float $x;
    public float $y;
}

buffer<Vec2> $points = buffer_new<Vec2>(1024);
$points[0]->x = 10.0;
$points[0]->y = 20.0;

echo buffer_len($points); // 1024
echo (int) $points[0]->x; // 10
```

Rules in v1:
- `packed class` fields must be POD scalars (`int`, `float`, `bool`), `ptr`, or other `packed class` types.
- `buffer<T>` only accepts POD scalars, `ptr`, or `packed class` element types.
- Allocate buffers explicitly with `buffer_new<T>(len)`.
- Read scalar elements with `$buf[$i]`; access packed elements with `$buf[$i]->field`.
- `buffer_len($buf)` returns the logical element count.
- Bounds checks are always enabled; out-of-range access aborts with a fatal error.
- `packed class` is metadata-only in v1: no inheritance, traits, methods, constructors, magic methods, or non-`public` fields.
- `buffer<T>` is fixed-size in v1: no push/pop, no implicit conversion to PHP arrays, and no copy-on-write behavior.
- This syntax is an elephc extension and is not valid in standard PHP.

See `examples/hot-path` for a runnable sample and `benchmarks/hot-path-buffer-vs-arrays` for a simple micro-benchmark.

## Operators

### Arithmetic

| Operator | Example | Notes |
|---|---|---|
| `+` | `$a + $b` | Addition |
| `-` | `$a - $b` | Subtraction |
| `*` | `$a * $b` | Multiplication |
| `/` | `$a / $b` | Division (always returns float) |
| `%` | `$a % $b` | Modulo |
| `**` | `$a ** $b` | Exponentiation (right-associative, returns float) |
| `-$x` | `-$x` | Unary negation |

### Comparison

| Operator | Example | Notes |
|---|---|---|
| `==` | `$a == $b` | Loose equality (cross-type: coerces to int) |
| `!=` | `$a != $b` | Inequality |
| `===` | `$a === $b` | Strict equality (type and value) |
| `!==` | `$a !== $b` | Strict inequality (type or value differs) |
| `<` | `$a < $b` | Less than |
| `>` | `$a > $b` | Greater than |
| `<=` | `$a <= $b` | Less than or equal |
| `>=` | `$a >= $b` | Greater than or equal |
| `<=>` | `$a <=> $b` | Spaceship: returns -1, 0, or 1 |

### Bitwise

| Operator | Example | Notes |
|---|---|---|
| `&` | `$a & $b` | Bitwise AND |
| `\|` | `$a \| $b` | Bitwise OR |
| `^` | `$a ^ $b` | Bitwise XOR |
| `~` | `~$a` | Bitwise NOT (complement) |
| `<<` | `$a << $b` | Left shift |
| `>>` | `$a >> $b` | Arithmetic right shift (preserves sign) |

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
| `.` | `"pi=" . 3.14` | Auto-coerces float to string |

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

### List Unpacking

Destructure an array into individual variables:

```php
<?php
[$a, $b, $c] = [10, 20, 30];
echo $a;  // 10
echo $b;  // 20
echo $c;  // 30

$arr = ["hello", "world"];
[$x, $y] = $arr;
echo $x . " " . $y;  // hello world
```

**Limitations:**
- All elements in the list pattern must be variables (no nested patterns or skipping with commas).
- The right-hand side must be an indexed array.

### Null Coalescing

```php
$x = null;
echo $x ?? "default";    // prints "default"
echo $x ?? $y ?? "last"; // chained — right-associative
```

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

### Type Casting

```php
$i = (int)3.7;       // 3
$f = (float)42;      // 42.0
$s = (string)42;     // "42"
$b = (bool)0;        // false
$a = (array)42;      // [42]
```

Aliases: `(integer)`, `(double)`, `(real)`, `(boolean)`.

**Not supported yet:** `?:` (short ternary).

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

// With key binding (indexed arrays)
foreach ($arr as $i => $value) {
    echo "$i: $value\n";
}

// With key binding (associative arrays)
$map = ["name" => "Alice", "age" => "30"];
foreach ($map as $key => $value) {
    echo "$key = $value\n";
}
```

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

### switch / case / default

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
    case 3:
        echo "three";
        break;
    default:
        echo "other";
        break;
}
// Output: two
```

Fall-through (no `break`) executes subsequent cases:

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
// Output: one or two
```

### match expression

PHP 8 style match expression. No fall-through, returns a value, uses strict comparison (`===`).

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

### try / catch / finally / throw

Exceptions work with object values:

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

- built-in `Exception` class and built-in `Throwable` interface are available without declaring them yourself
- built-in `Exception` currently provides a minimal PHP-style API: public `$message`, `__construct($message = "")`, and `getMessage()`
- `throw <expr>;` where `<expr>` has an object type implementing `Throwable`
- `throw <expr>` can also be used inside expressions such as `??` and ternaries
- `try { ... } catch (ClassName $e) { ... }`
- `catch (TypeA | TypeB $e)` for PHP-style multi-catch
- `catch (Exception)` without binding the exception variable
- catch types must extend or implement `Throwable`
- `catch (Throwable $e)` matches objects implementing the built-in `Throwable` interface, including the built-in `Exception`
- multiple `catch` clauses
- `try { ... } finally { ... }`
- `return`, `break`, and `continue` run enclosing `finally` blocks before leaving the `try`

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

### Default parameter values

```php
<?php
function greet($name = "world") {
    echo "Hello " . $name . "\n";
}
greet();        // Hello world
greet("PHP");   // Hello PHP

function add($a, $b = 0, $c = 0) {
    return $a + $b + $c;
}
echo add(5);       // 5
echo add(5, 3);    // 8
echo add(5, 3, 2); // 10
```

Parameters with defaults must come after required parameters. When calling, you can omit trailing arguments that have defaults.

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

### Anonymous functions (closures)

```php
<?php
$double = function($x) {
    return $x * 2;
};
echo $double(5); // 10

// Passing closures to array functions
$nums = [1, 2, 3, 4];
$doubled = array_map(function($n) { return $n * 2; }, $nums);
// $doubled = [2, 4, 6, 8]
```

Closures can capture variables from the enclosing scope with `use`:

```php
<?php
$factor = 3;
$multiply = function($x) use ($factor) {
    return $x * $factor;
};
echo $multiply(5); // 15

$a = 10;
$b = 20;
$sum = function() use ($a, $b) { return $a + $b; };
echo $sum(); // 30
```

**Limitation:** Closures with `use` captures work for direct calls (`$fn(args)`) but cannot be passed to `array_map`, `array_filter`, etc. — those built-ins call the closure internally without the hidden capture arguments.

### Arrow functions

```php
<?php
$double = fn($x) => $x * 2;
echo $double(5); // 10

// Arrow functions with array callbacks
$nums = [1, 2, 3, 4];
$squared = array_map(fn($n) => $n * $n, $nums);
// $squared = [1, 4, 9, 16]

usort($nums, fn($a, $b) => $b - $a);
// $nums = [4, 3, 2, 1]
```

Arrow functions are single-expression closures — the body is implicitly returned, no `return` keyword needed.

### Global variables

The `global` keyword allows a function to access and modify variables from the main (top-level) scope:

```php
<?php
$x = 10;
function test() {
    global $x;
    echo $x;    // 10
    $x = 20;
}
test();
echo $x;        // 20
```

Multiple variables can be declared global in one statement: `global $a, $b;`

### Static variables

Static variables retain their value between function calls:

```php
<?php
function counter() {
    static $n = 0;
    $n++;
    echo $n . "\n";
}
counter(); // 1
counter(); // 2
counter(); // 3
```

The initial value is evaluated only once, on the first call. Each function has its own independent static variable namespace.

### Pass by reference

Parameters prefixed with `&` receive a reference to the caller's variable, allowing the function to modify it:

```php
<?php
function increment(&$val) {
    $val++;
}
$x = 5;
increment($x);
echo $x; // 6

function swap(&$a, &$b) {
    $tmp = $a;
    $a = $b;
    $b = $tmp;
}
$p = 1;
$q = 2;
swap($p, $q);
echo $p; // 2
echo $q; // 1
```

Reference parameters can be mixed with regular parameters: `function foo(&$ref, $val) { }`

### Variadic Functions

A variadic parameter collects all remaining arguments into an array:

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
echo sum();         // 0
```

Variadic parameters can be combined with regular parameters. The variadic parameter must be the last one:

```php
<?php
function greet($greeting, ...$names) {
    foreach ($names as $name) {
        echo $greeting . " " . $name . "\n";
    }
}
greet("Hello", "Alice", "Bob");
```

### Spread Operator

The spread operator (`...`) unpacks an array into individual arguments:

```php
<?php
$args = [10, 20, 30];
echo sum(...$args); // 60
```

Spread can also be used in array literals to merge arrays:

```php
<?php
$a = [1, 2];
$b = [3, 4];
$c = [...$a, ...$b];      // [1, 2, 3, 4]
$d = [...$a, 5, 6, ...$b]; // [1, 2, 5, 6, 3, 4]
```

### Limitations

- Closures with `use` captures cannot be passed to `array_map`, `array_filter`, etc. (only direct `$fn()` calls)

## Print

`print` is a language construct that works as an alias for `echo`:

```php
<?php
$name = "World";
print "Hello, $name\n";
print 42;
```

Like PHP, `print` always returns 1, but elephc does not support using `print` as an expression (e.g., `$x = print "hello";` is not supported).

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

Variable interpolation in double-quoted strings:

```php
<?php
$name = "World";
echo "Hello, $name\n";          // prints: Hello, World
echo "Hello, " . $name . "\n";  // also works (concatenation)
```

### Heredoc strings

Multi-line strings with escape sequence processing (like double-quoted):

```php
<?php
echo <<<EOT
Hello World
This is line 2
EOT;
```

### Nowdoc strings

Multi-line strings without escape processing (like single-quoted):

```php
<?php
echo <<<'EOT'
Hello World
No escapes: \n \t stay literal
EOT;
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

### String indexing

String indexing is supported as one-character slice syntax on strings:

```php
<?php
$s = "hello";
echo $s[1];    // e
echo $s[-1];   // o
echo "[" . $s[99] . "]";  // []
```

The index must be an integer. Negative indices count from the end of the string, and out-of-bounds indices evaluate to an empty string. Read access is supported; string offset assignment such as `$s[0] = "x"` is not supported.

### String arrays

```php
<?php
$names = ["Alice", "Bob", "Charlie"];
foreach ($names as $name) {
    echo "Hello, " . $name . "\n";
}
```

### Associative arrays

```php
<?php
$map = ["name" => "Alice", "city" => "Paris"];
echo $map["name"];       // Alice

$map["age"] = "30";      // add new key
echo $map["age"];         // 30

foreach ($map as $key => $value) {
    echo "$key: $value\n";
}
```

Associative arrays use a hash table runtime. Keys follow the type inferred from the first key expression (commonly strings, but integer keys are also accepted when used consistently). If later values do not match the first value type, the checker widens the associative-array value type to the internal `mixed` runtime shape and the hash stores a per-entry value tag. Iteration-based operations such as `foreach`, `array_keys()`, `array_values()`, `array_search()`, `in_array()`, and `json_encode()` preserve insertion order and dispatch on each entry's runtime tag like PHP.

### Copy-on-write semantics

Indexed and associative arrays are **shared until modified**, matching PHP's ordinary by-value behavior for arrays:

```php
<?php
$a = [1, 2];
$b = $a;      // shares the same backing storage initially
$b[0] = 9;    // first write detaches $b

echo $a[0];   // 1
echo $b[0];   // 9
```

The same split-on-write rule applies when arrays are passed to ordinary (non-`&`) parameters and when mutating built-ins such as `array_push()`, `sort()`, `shuffle()`, `array_shift()`, `array_unshift()`, and `array_splice()` operate on a shared array value. Nested arrays are still shallow-shared until the nested container itself is written to.

### Multi-dimensional arrays

```php
<?php
$matrix = [[1, 2], [3, 4]];
echo $matrix[0][1];    // 2
echo $matrix[1][0];    // 3

// Nested foreach
foreach ($matrix as $row) {
    foreach ($row as $val) {
        echo $val . " ";
    }
    echo "\n";
}
```

### Limitations

- No array union operator (`+`)
- Indexed arrays are still homogeneous, except that object elements may widen to a shared parent class when all entries are related by inheritance (for example `Dog` + `Cat` can infer `Animal[]`)

## FFI

elephc supports direct calls into C libraries through `extern` declarations.

### Extern functions

```php
<?php
extern function atoi(string $s): int;
extern function signal(int $sig, callable $handler): ptr;
extern function raise(int $sig): int;
```

Supported FFI types:

| FFI type | C shape | Notes |
|---|---|---|
| `int` | integer / long | Passed in integer registers |
| `float` | `double` | Passed in floating-point registers |
| `string` | `char *` | Copied to a temporary null-terminated C string for the duration of the native call; if C needs to retain the pointer, declare the boundary as `ptr` instead |
| `bool` | integer `0` / `1` | Passed as an integer register |
| `void` | no value | Valid only as a return type |
| `ptr` | `void *` | Opaque pointer |
| `ptr<Name>` | typed pointer | Still ABI-compatible with a raw pointer |
| `callable` | function pointer | Pass a user-defined elephc function by string name |

### Extern blocks

```php
<?php
extern "System" {
    function getenv(string $name): string;
    function strlen(string $s): int;
}
```

Libraries declared in an `extern "lib"` block are added automatically to the linker command as `-l<lib>`.

FFI string ownership follows two explicit defaults:

- `string` parameters are **borrowed call-scoped C strings**. elephc creates a temporary null-terminated copy before the native call and frees it immediately after the call returns.
- `string` return values are **borrowed `char *` results**. elephc copies the bytes into an owned elephc string right away, so the native side retains ownership of the original pointer.

If an API expects the native side to keep a string/buffer beyond the call boundary, prefer declaring that boundary as `ptr` (or using a shim) instead of `string`.

This makes direct bindings to native libraries practical for simple APIs. For example, ordinary libc allocation routines can be declared and used without special compiler support:

```php
<?php
extern "System" {
    function malloc(int $size): ptr;
    function free(ptr $p): void;
    function memset(ptr $dest, int $byte, int $count): ptr;
}

$buf = malloc(16);
memset($buf, 0, 16);
free($buf);
```

### Extern globals

```php
<?php
extern global ptr $environ;
echo ptr_is_null($environ) ? "missing" : "ok";
```

`extern global` reads and writes the actual C symbol, not an elephc-managed shadow copy.

### Extern classes

```php
<?php
extern class Point {
    public int $x;
    public int $y;
}
```

Extern classes describe flat C struct layouts for FFI type checking. Field sizes follow the declared C-facing types, so `string` fields are treated as a single pointer-sized `char *`.

Typed extern pointers can be dereferenced with `ptr_cast<T>()` plus normal property syntax for flat layouts:

```php
<?php
extern class Point {
    public int $x;
    public int $y;
}

extern "System" {
    function malloc(int $size): ptr;
    function free(ptr $p): void;
}

$mem = malloc(ptr_sizeof("Point"));
$pt = ptr_cast<Point>($mem);
$pt->x = 10;
$pt->y = 20;
echo $pt->x; // 10
free($mem);
```

### Callback functions

```php
<?php
extern function signal(int $sig, callable $handler): ptr;
extern function raise(int $sig): int;

function on_signal($sig) {
    echo $sig;
}

signal(15, "on_signal");
raise(15);
```

Callback rules:

- Pass callbacks by string name, for example `"on_signal"`.
- Callback functions cannot be variadic, cannot use default values, and cannot use pass-by-reference parameters.
- Only C-compatible callback shapes are supported today: `int`, `float`, `bool`, `ptr`, and `void`.
- String callback parameters and string callback return values are not supported.

## Built-in Functions

### String functions

| Function | Signature | Description |
|---|---|---|
| `strlen()` | `strlen($str): int` | Returns string length |
| `number_format()` | `number_format($n [, $dec [, $dec_point, $thou_sep]]): string` | Format number with separators |
| `substr()` | `substr($str, $start [, $len]): string` | Extract substring |
| `strpos()` | `strpos($hay, $needle): int` | Find first occurrence. Returns `-1` if not found (PHP returns `false`) |
| `strrpos()` | `strrpos($hay, $needle): int` | Find last occurrence (-1 if not found) |
| `strstr()` | `strstr($hay, $needle): string` | Find first occurrence and return rest |
| `str_replace()` | `str_replace($search, $replace, $subject): string` | Replace all occurrences |
| `str_ireplace()` | `str_ireplace($search, $replace, $subject): string` | Case-insensitive replace |
| `substr_replace()` | `substr_replace($str, $repl, $start [, $len]): string` | Replace substring |
| `strtolower()` | `strtolower($str): string` | Convert to lowercase |
| `strtoupper()` | `strtoupper($str): string` | Convert to uppercase |
| `ucfirst()` | `ucfirst($str): string` | Uppercase first character |
| `lcfirst()` | `lcfirst($str): string` | Lowercase first character |
| `ucwords()` | `ucwords($str): string` | Uppercase first letter of each word |
| `trim()` | `trim($str [, $chars]): string` | Strip whitespace (or custom chars) from both ends |
| `ltrim()` | `ltrim($str [, $chars]): string` | Strip whitespace (or custom chars) from left |
| `rtrim()` | `rtrim($str [, $chars]): string` | Strip whitespace (or custom chars) from right |
| `str_repeat()` | `str_repeat($str, $times): string` | Repeat a string |
| `str_pad()` | `str_pad($str, $len [, $pad, $type]): string` | Pad string to length |
| `str_split()` | `str_split($str [, $len]): array` | Split into chunks |
| `strrev()` | `strrev($str): string` | Reverse a string |
| `strcmp()` | `strcmp($a, $b): int` | Binary-safe string comparison |
| `strcasecmp()` | `strcasecmp($a, $b): int` | Case-insensitive comparison |
| `str_contains()` | `str_contains($hay, $needle): bool` | Check if string contains substring |
| `str_starts_with()` | `str_starts_with($hay, $prefix): bool` | Check prefix |
| `str_ends_with()` | `str_ends_with($hay, $suffix): bool` | Check suffix |
| `ord()` | `ord($char): int` | ASCII value of first character |
| `chr()` | `chr($code): string` | Character from ASCII code |
| `explode()` | `explode($delim, $str): array` | Split string into array |
| `implode()` | `implode($glue, $arr): string` | Join array into string |
| `addslashes()` | `addslashes($str): string` | Escape quotes and backslashes |
| `stripslashes()` | `stripslashes($str): string` | Remove escape backslashes |
| `nl2br()` | `nl2br($str): string` | Insert `<br />` before newlines |
| `wordwrap()` | `wordwrap($str [, $width [, $break [, $cut]]]): string` | Wrap text at width |
| `bin2hex()` | `bin2hex($str): string` | Convert binary to hex |
| `hex2bin()` | `hex2bin($str): string` | Convert hex to binary |
| `sprintf()` | `sprintf($fmt, ...): string` | Format string (%s, %d, %f, %x, %e, %g, %o, %c, %%) with width, precision, padding, alignment, and sign modifiers |
| `printf()` | `printf($fmt, ...): int` | Format and print |
| `md5()` | `md5($str): string` | MD5 hash (32-char hex) |
| `sha1()` | `sha1($str): string` | SHA1 hash (40-char hex) |
| `hash()` | `hash($algo, $data): string` | Hash with algorithm (md5, sha1, sha256) |
| `sscanf()` | `sscanf($str, $fmt): array` | Parse string with format (%d, %s) |
| `htmlspecialchars()` | `htmlspecialchars($str): string` | Escape HTML special chars |
| `htmlentities()` | `htmlentities($str): string` | Alias for htmlspecialchars |
| `html_entity_decode()` | `html_entity_decode($str): string` | Decode HTML entities |
| `urlencode()` | `urlencode($str): string` | URL-encode (spaces as +) |
| `urldecode()` | `urldecode($str): string` | URL-decode |
| `rawurlencode()` | `rawurlencode($str): string` | URL-encode (spaces as %20) |
| `rawurldecode()` | `rawurldecode($str): string` | URL-decode (RFC 3986) |
| `base64_encode()` | `base64_encode($str): string` | Base64 encode |
| `base64_decode()` | `base64_decode($str): string` | Base64 decode |
| `ctype_alpha()` | `ctype_alpha($str): bool` | All chars are A-Z/a-z |
| `ctype_digit()` | `ctype_digit($str): bool` | All chars are 0-9 |
| `ctype_alnum()` | `ctype_alnum($str): bool` | All chars are alphanumeric |
| `ctype_space()` | `ctype_space($str): bool` | All chars are whitespace |
| `intval()` | `intval($val): int` | Converts to integer |

### Array functions

| Function | Signature | Description |
|---|---|---|
| `count()` | `count($arr): int` | Number of elements |
| `array_push()` | `array_push($arr, $val): void` | Add element to end of an indexed array |
| `array_pop()` | `array_pop($arr): mixed` | Remove and return last element |
| `in_array()` | `in_array($needle, $arr): int` | Search for value (returns 0/1) |
| `array_keys()` | `array_keys($arr): array` | Returns the array keys (ints for indexed arrays, strings for associative arrays) |
| `array_values()` | `array_values($arr): array` | Returns copy of values |
| `sort()` | `sort($arr): void` | Sort ascending (in-place) |
| `rsort()` | `rsort($arr): void` | Sort descending (in-place) |
| `isset()` | `isset($var): int` | Check if variable is defined (always 1) |
| **Searching** | | |
| `array_key_exists()` | `array_key_exists($key, $arr): bool` | Check if key exists in array |
| `array_search()` | `array_search($needle, $arr): int` | Search for value, return key. Returns `-1` if not found (PHP returns `false`) |
| **Slicing** | | |
| `array_slice()` | `array_slice($arr, $offset [, $length]): array` | Extract a slice of the array |
| `array_splice()` | `array_splice($arr, $offset [, $length]): array` | Remove/replace part of array |
| `array_chunk()` | `array_chunk($arr, $size): array` | Split array into chunks |
| **Combining** | | |
| `array_merge()` | `array_merge($arr1, $arr2): array` | Merge two arrays |
| `array_combine()` | `array_combine($keys, $values): array` | Create array using keys and values arrays |
| `array_fill()` | `array_fill($start, $num, $value): array` | Fill array with values |
| `array_fill_keys()` | `array_fill_keys($keys, $value): array` | Fill array with values using keys |
| `array_pad()` | `array_pad($arr, $size, $value): array` | Pad array to specified length |
| `range()` | `range($start, $end): array` | Create array of sequential integers |
| **Filtering** | | |
| `array_diff()` | `array_diff($arr1, $arr2): array` | Values in $arr1 not in $arr2 |
| `array_intersect()` | `array_intersect($arr1, $arr2): array` | Values present in both arrays |
| `array_diff_key()` | `array_diff_key($arr1, $arr2): array` | Keys in $arr1 not in $arr2 |
| `array_intersect_key()` | `array_intersect_key($arr1, $arr2): array` | Keys present in both arrays |
| `array_unique()` | `array_unique($arr): array` | Remove duplicate values |
| **Transforming** | | |
| `array_reverse()` | `array_reverse($arr): array` | Return array in reverse order |
| `array_flip()` | `array_flip($arr): array` | Exchange keys and values |
| `array_shift()` | `array_shift($arr): mixed` | Remove and return first element |
| `array_unshift()` | `array_unshift($arr, $value): int` | Prepend element to array |
| **Aggregating** | | |
| `array_sum()` | `array_sum($arr): int\|float` | Sum of all values |
| `array_product()` | `array_product($arr): int\|float` | Product of all values |
| **Sorting** | | |
| `asort()` | `asort($arr): void` | Sort by value, maintain key association |
| `arsort()` | `arsort($arr): void` | Sort by value descending, maintain key association |
| `ksort()` | `ksort($arr): void` | Sort by key ascending |
| `krsort()` | `krsort($arr): void` | Sort by key descending |
| `natsort()` | `natsort($arr): void` | Natural order sort |
| `natcasesort()` | `natcasesort($arr): void` | Case-insensitive natural order sort |
| **Random** | | |
| `shuffle()` | `shuffle($arr): void` | Randomly shuffle array (in-place) |
| `array_rand()` | `array_rand($arr): int` | Pick one random key |
| **Multi-dimensional** | | |
| `array_column()` | `array_column($arr, $column_key): array` | Extract column from an indexed array of associative rows |
| **Callback-based** | | |
| `array_map()` | `array_map("callback", $arr): array` | Apply callback to each element, return new array |
| `array_filter()` | `array_filter($arr, "callback"): array` | Filter elements where callback returns truthy |
| `array_reduce()` | `array_reduce($arr, "callback", $init): int` | Reduce array to a single accumulator value via callback. In the current checker/runtime subset this accumulator is typed as `int` rather than PHP's fully mixed result |
| `array_walk()` | `array_walk($arr, "callback"): void` | Call callback on each element (side-effects) |
| `usort()` | `usort($arr, "callback"): void` | Sort array using user comparison function |
| `uksort()` | `uksort($arr, "callback"): void` | Sort array with a user comparison callback (currently routed through the same runtime sort path as `usort`) |
| `uasort()` | `uasort($arr, "callback"): void` | Sort array with a user comparison callback (currently routed through the same runtime sort path as `usort`) |
| **Function handling** | | |
| `call_user_func()` | `call_user_func("name", ...): mixed` | Call a function by name with arguments |
| `call_user_func_array()` | `call_user_func_array("name", $args): mixed` | Call a function with arguments from an array |
| `function_exists()` | `function_exists("name"): bool` | Check if a function is defined |
> **Note:** Callback arguments can be string literals containing the function name (e.g., `"double"`), anonymous functions, or arrow functions. String-literal callback names are resolved with the same namespace rules as ordinary function calls, including the current namespace and `use function` aliases. Closure support is documented separately below, including the `use (...)` limitation.

> **Type checker note:** `array_push()` only accepts indexed arrays as its first argument, and `array_column()` requires an indexed array whose elements are associative arrays.

**Not yet supported:** `compact()`, `extract()` (require dynamic variables).

### Math functions

| Function | Signature | Description |
|---|---|---|
| `abs()` | `abs($val): int\|float` | Absolute value (preserves type) |
| `floor()` | `floor($val): float` | Round down |
| `ceil()` | `ceil($val): float` | Round up |
| `round()` | `round($val [, $precision]): float` | Round to nearest (optional decimal places) |
| `sqrt()` | `sqrt($val): float` | Square root |
| `pow()` | `pow($base, $exp): float` | Exponentiation |
| `min()` | `min($a, $b, ...): int\|float` | Minimum of two or more values (variadic) |
| `max()` | `max($a, $b, ...): int\|float` | Maximum of two or more values (variadic) |
| `intdiv()` | `intdiv($a, $b): int` | Integer division |
| `fmod()` | `fmod($a, $b): float` | Float modulo |
| `fdiv()` | `fdiv($a, $b): float` | Float division (returns INF for /0) |
| `rand()` | `rand([$min, $max]): int` | Random integer (0 or 2 args) |
| `mt_rand()` | `mt_rand([$min, $max]): int` | Alias for rand() |
| `random_int()` | `random_int($min, $max): int` | Cryptographic random |
| `sin()` | `sin($angle): float` | Sine (radians) |
| `cos()` | `cos($angle): float` | Cosine (radians) |
| `tan()` | `tan($angle): float` | Tangent (radians) |
| `asin()` | `asin($val): float` | Arc sine |
| `acos()` | `acos($val): float` | Arc cosine |
| `atan()` | `atan($val): float` | Arc tangent |
| `atan2()` | `atan2($y, $x): float` | Two-argument arc tangent |
| `sinh()` | `sinh($val): float` | Hyperbolic sine |
| `cosh()` | `cosh($val): float` | Hyperbolic cosine |
| `tanh()` | `tanh($val): float` | Hyperbolic tangent |
| `log()` | `log($num [, $base]): float` | Logarithm (natural or custom base) |
| `log2()` | `log2($num): float` | Base-2 logarithm |
| `log10()` | `log10($num): float` | Base-10 logarithm |
| `exp()` | `exp($val): float` | e^x |
| `hypot()` | `hypot($x, $y): float` | Hypotenuse: sqrt(x² + y²) |
| `deg2rad()` | `deg2rad($degrees): float` | Degrees to radians |
| `rad2deg()` | `rad2deg($radians): float` | Radians to degrees |
| `pi()` | `pi(): float` | Returns M_PI |

### Type functions

| Function | Signature | Description |
|---|---|---|
| `is_null()` | `is_null($val): bool` | Returns true if null |
| `is_float()` | `is_float($val): bool` | Returns true if float |
| `is_int()` | `is_int($val): bool` | Returns true if integer |
| `is_string()` | `is_string($val): bool` | Returns true if string |
| `is_numeric()` | `is_numeric($val): bool` | Returns true if int or float |
| `is_bool()` | `is_bool($val): bool` | Returns true if bool |
| `is_nan()` | `is_nan($val): bool` | Returns true if NAN |
| `is_finite()` | `is_finite($val): bool` | Returns true if not INF/NAN |
| `is_infinite()` | `is_infinite($val): bool` | Returns true if INF or -INF |
| `boolval()` | `boolval($val): bool` | Convert to bool |
| `floatval()` | `floatval($val): float` | Convert to float |
| `gettype()` | `gettype($val): string` | Returns type name (`"integer"`, `"double"`, `"string"`, `"boolean"`, `"NULL"`, `"array"`, `"callable"`, `"object"`, or `"pointer"`) |
| `empty()` | `empty($val): bool` | Returns true if value is falsy (0, 0.0, "", false, null, empty array) |
| `unset()` | `unset($var): void` | Sets variable to null |
| `settype()` | `settype($var, $type): bool` | Changes variable type in place |

### System functions

| Function | Signature | Description |
|---|---|---|
| `exit()` | `exit($code = 0): void` | Terminate program |
| `die()` | `die($code = 0): void` | Alias for `exit()` |
| `define()` | `define("NAME", value): void` | Define a named constant |
| `time()` | `time(): int` | Get current Unix timestamp |
| `microtime()` | `microtime($as_float = false): float` | Get current time with microsecond precision. elephc currently always returns a float, even when PHP would return a string for `false`. |
| `sleep()` | `sleep($seconds): int` | Sleep for given seconds |
| `usleep()` | `usleep($microseconds): void` | Sleep for given microseconds |
| `getenv()` | `getenv($name): string` | Get environment variable value |
| `putenv()` | `putenv($assignment): bool` | Set environment variable (format: "KEY=VALUE") |
| `php_uname()` | `php_uname([$mode]): string` | Get OS name (returns `"Darwin"`; optional mode argument is accepted) |
| `phpversion()` | `phpversion(): string` | Get elephc version string |
| `exec()` | `exec($command): string` | Execute command, return output |
| `shell_exec()` | `shell_exec($command): string` | Execute command via shell, return full output |
| `system()` | `system($command): string` | Execute command, output to stdout |
| `passthru()` | `passthru($command): void` | Execute command, pass raw output to stdout |
| `date()` | `date($format [, $timestamp]): string` | Format a Unix timestamp. Format chars: Y, m, d, H, i, s, l, F, D, M, N, j, n, G, g, A, a, U. If the timestamp is omitted, uses current time. |
| `mktime()` | `mktime($h, $m, $s, $mon, $day, $yr): int` | Create Unix timestamp from components |
| `strtotime()` | `strtotime($datetime): int` | Parse "YYYY-MM-DD" or "YYYY-MM-DD HH:MM:SS" to timestamp |
| `json_encode()` | `json_encode($value): string` | Encode value as JSON. Supports int, float, string, bool, null, indexed arrays, associative arrays, and boxed `mixed` payloads. Heterogeneous indexed arrays are emitted through runtime tag dispatch rather than a single compile-time element encoder. |
| `json_decode()` | `json_decode($json): string` | Decode a JSON string value (strips quotes, unescapes). Returns string representation. |
| `json_last_error()` | `json_last_error(): int` | Always returns 0 (JSON_ERROR_NONE) |
| `preg_match()` | `preg_match($pattern, $subject): int` | Test if regex matches subject. Returns 1 or 0. Uses POSIX extended regex via libc. |
| `preg_match_all()` | `preg_match_all($pattern, $subject): int` | Count all non-overlapping regex matches |
| `preg_replace()` | `preg_replace($pattern, $replacement, $subject): string` | Replace all regex matches with replacement string |
| `preg_split()` | `preg_split($pattern, $subject): array` | Split string by regex pattern, returns string array |

### I/O functions

| Function | Signature | Description |
|---|---|---|
| `fopen()` | `fopen($filename, $mode): int` | Open a file and return an integer file descriptor (modes: r, w, a, r+, w+, a+) |
| `fclose()` | `fclose($handle): bool` | Close a file handle |
| `fread()` | `fread($handle, $length): string` | Read up to $length bytes |
| `fwrite()` | `fwrite($handle, $data): int` | Write string to file, returns bytes written |
| `fgets()` | `fgets($handle): string` | Read a line from file or STDIN |
| `feof()` | `feof($handle): bool` | Check if end-of-file reached |
| `readline()` | `readline([$prompt]): string` | Read a line from STDIN |
| `fseek()` | `fseek($handle, $offset [, $whence]): int` | Seek to position in file |
| `ftell()` | `ftell($handle): int` | Get current position in file |
| `rewind()` | `rewind($handle): bool` | Seek to beginning of file |
| `fgetcsv()` | `fgetcsv($handle [, $sep]): array` | Read a CSV line into array |
| `fputcsv()` | `fputcsv($handle, $fields [, $sep]): int` | Write array as CSV line |
| `file_get_contents()` | `file_get_contents($filename): string` | Read entire file into string |
| `file_put_contents()` | `file_put_contents($filename, $data): int` | Write string to file |
| `file()` | `file($filename): array` | Read file into array of lines |
| `file_exists()` | `file_exists($filename): bool` | Check if file or directory exists |
| `is_file()` | `is_file($filename): bool` | Check if path is a regular file |
| `is_dir()` | `is_dir($filename): bool` | Check if path is a directory |
| `is_readable()` | `is_readable($filename): bool` | Check if file is readable |
| `is_writable()` | `is_writable($filename): bool` | Check if file is writable |
| `filesize()` | `filesize($filename): int` | Get file size in bytes |
| `filemtime()` | `filemtime($filename): int` | Get file modification time (Unix timestamp) |
| `copy()` | `copy($source, $dest): bool` | Copy a file |
| `rename()` | `rename($old, $new): bool` | Rename/move a file |
| `unlink()` | `unlink($filename): bool` | Delete a file |
| `mkdir()` | `mkdir($pathname): bool` | Create a directory |
| `rmdir()` | `rmdir($pathname): bool` | Remove a directory |
| `scandir()` | `scandir($directory): array` | List files in directory |
| `glob()` | `glob($pattern): array` | Find files matching pattern |
| `getcwd()` | `getcwd(): string` | Get current working directory |
| `chdir()` | `chdir($directory): bool` | Change working directory |
| `tempnam()` | `tempnam($dir, $prefix): string` | Create a temporary file name |
| `sys_get_temp_dir()` | `sys_get_temp_dir(): string` | Get system temporary directory |

### Debugging functions

| Function | Signature | Description |
|---|---|---|
| `var_dump()` | `var_dump($value): void` | Output type and value for debugging |
| `print_r()` | `print_r($value): void` | Print human-readable representation |

```php
<?php
$arr = [1, 2, 3];
var_dump($arr);
// array(3) {
//   [0]=> int(1)
//   [1]=> int(2)
//   [2]=> int(3)
// }

print_r($arr);
// Array
// (
//     [0] => 1
//     [1] => 2
//     [2] => 3
// )
```

### Pointer functions

| Function | Signature | Description |
|---|---|---|
| `ptr()` | `ptr($var): pointer` | Take the address of a variable lvalue |
| `ptr_null()` | `ptr_null(): pointer` | Create a null pointer (`0x0`) |
| `ptr_is_null()` | `ptr_is_null($p): bool` | Check if pointer is null |
| `ptr_get()` | `ptr_get($p): int` | Read one 8-byte machine word at pointer address |
| `ptr_set()` | `ptr_set($p, $val): void` | Write one 8-byte machine word (`int`, `bool`, `null`, or `pointer`) |
| `ptr_read8()` | `ptr_read8($p): int` | Read one byte and zero-extend it to an integer |
| `ptr_read32()` | `ptr_read32($p): int` | Read one 32-bit word and zero-extend it to an integer |
| `ptr_write8()` | `ptr_write8($p, $val): void` | Write the low 8 bits of an integer |
| `ptr_write32()` | `ptr_write32($p, $val): void` | Write the low 32 bits of an integer |
| `ptr_offset()` | `ptr_offset($p, $bytes): pointer` | Pointer arithmetic (add byte offset) |
| `ptr_cast<T>()` | `ptr_cast<Type>($p): pointer` | Change pointer type tag (same address, validated target type) |
| `ptr_sizeof()` | `ptr_sizeof("type"): int` | Return byte size of a known builtin type or declared class |

```php
<?php
$x = 42;
$p = ptr($x);           // take address of $x
echo $p;                 // prints "0x16f502348" (hex address)
echo ptr_get($p);        // prints "42"
ptr_set($p, 99);         // write through pointer
echo $x;                 // prints "99" — variable was modified

echo ptr_sizeof("int");  // 8
echo ptr_sizeof("string"); // 16

$null = ptr_null();
echo ptr_is_null($null); // 1

// Pointer comparison with === and !==
$a = ptr_null();
$b = ptr_null();
echo $a === $b;          // 1 (same address)
```

Raw off-heap buffers can be accessed byte-by-byte or word-by-word:

```php
<?php
extern "System" {
    function malloc(int $size): ptr;
    function free(ptr $p): void;
}

$buf = malloc(4);
ptr_write8($buf, 255);
ptr_write32($buf, 305419896);
echo ptr_read8($buf);
echo ptr_read32($buf);
free($buf);
```

Notes:
- `ptr()` only accepts variables. `ptr(1 + 2)` is a compile-time error.
- `ptr_get()` and `ptr_set()` only accept pointers. Dereferencing `ptr_null()` aborts with `Fatal error: null pointer dereference`.
- `ptr_set()` currently writes a single 8-byte word. It is intended for `int`, `bool`, `null`, and pointer values.
- `ptr_read8()`, `ptr_read32()`, `ptr_write8()`, and `ptr_write32()` are intended for raw buffers and packed native data.
- `ptr_cast<T>()` preserves the address and only changes the static pointer tag. `T` must be a known builtin pointee type (`int`, `float`, `bool`, `string`, `ptr`) or a declared class, `packed class`, or `extern class` name.
- `ptr_sizeof()` accepts builtin pointee names plus declared PHP class names and `extern class` names.
- Use `===` and `!==` for pointer comparison. Loose comparison with `==` / `!=` is rejected.

## Constants

### User-defined constants

Constants can be defined with `const` or `define()`. They are resolved at compile time and are globally accessible, including inside functions.

```php
<?php
const MAX_RETRIES = 3;
const APP_NAME = "elephc";

define("PI", 3.14159);
define("GREETING", "Hello");

echo APP_NAME;    // elephc
echo PI;          // 3.14159

function show() {
    echo MAX_RETRIES;  // constants are global
}
show();
```

**Limitations:**
- Constant values must be literals (int, float, string, bool, null). Expressions are not supported as constant values.
- `define()` first argument must be a string literal (not a variable).

### Predefined constants

| Constant | Type | Value |
|---|---|---|
| `INF` | float | Positive infinity |
| `NAN` | float | Not a Number |
| `PHP_INT_MAX` | int | 9223372036854775807 |
| `PHP_INT_MIN` | int | -9223372036854775808 |
| `PHP_FLOAT_MAX` | float | ~1.8e308 |
| `PHP_FLOAT_MIN` | float | ~2.2e-308 |
| `PHP_FLOAT_EPSILON` | float | ~2.2e-16 |
| `M_PI` | float | 3.14159265358979... |
| `M_E` | float | 2.71828182845904... |
| `M_SQRT2` | float | 1.41421356237309... |
| `M_PI_2` | float | 1.57079632679489... |
| `M_PI_4` | float | 0.78539816339744... |
| `M_LOG2E` | float | 1.44269504088896... |
| `M_LOG10E` | float | 0.43429448190325... |
| `PHP_EOL` | string | End of line character (`"\n"`) |
| `PHP_OS` | string | Operating system name (`"Darwin"`) |
| `DIRECTORY_SEPARATOR` | string | Directory separator (`"/"`) |
| `STDIN` | int | Standard input file descriptor (`0`) |
| `STDOUT` | int | Standard output file descriptor (`1`) |
| `STDERR` | int | Standard error file descriptor (`2`) |

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

## Include / Require

```php
<?php
include 'helpers.php';
require 'config.php';
include_once 'utils.php';
require_once 'lib.php';
```

All four forms are resolved **at compile time** — the included file's code is inlined at the point of inclusion. Paths are relative to the including file's directory.

| Form | Missing file | Already included |
|---|---|---|
| `include` | Skipped (compile continues) | Re-included |
| `require` | Compile error | Re-included |
| `include_once` | Skipped | Skipped |
| `require_once` | Compile error | Skipped |

Both `include 'f';` and `include('f');` syntax are supported.

**Limitations:**
- Path must be a string literal (no variables or expressions)
- Included files must start with `<?php`

## Namespaces

elephc supports PHP-style namespaces for functions, classes, interfaces, traits, and constants.

### Declaring a namespace

You can declare a file-wide namespace with a semicolon:

```php
<?php
namespace App\Core;

function version() {
    return "1.0";
}
```

You can also use block form:

```php
<?php
namespace App\Core {
    class Clock {
        public static function now() {
            return "tick";
        }
    }
}
```

### Importing names with `use`

```php
<?php
namespace App\Http;

use App\Support\Response;
use function App\Support\render as render_page;
use const App\Support\STATUS_OK;

echo STATUS_OK;
$resp = new Response();
echo render_page("home");
```

Supported import forms:

- `use Foo\Bar;`
- `use Foo\Bar as Baz;`
- `use function Foo\bar;`
- `use function Foo\bar as baz;`
- `use const Foo\BAR;`
- group use: `use Vendor\Pkg\{Thing, Other as Alias};`
- mixed group use kinds: `use Vendor\Pkg\{function render, const VERSION, Tool};`

### Name resolution rules

- Unqualified class-like names (`Thing`) honor `use` aliases, otherwise resolve relative to the current namespace.
- Unqualified function and constant names first honor `use function` / `use const` aliases, then try the current namespace, then fall back to the global symbol if no namespaced declaration exists.
- Fully-qualified names like `\Lib\Tool` always refer to the global canonical name directly.
- Included files keep their own declared namespace. `require "lib.php";` does not force the included file into the caller's namespace.

### Namespaces and callbacks

String-literal callback names passed to namespace-aware builtins such as `function_exists()`, `call_user_func()`, `call_user_func_array()`, `array_map()`, `array_filter()`, `array_reduce()`, `array_walk()`, `usort()`, `uksort()`, and `uasort()` follow the same resolution rules as normal function calls.

### Known limitations in current JSON / regex / date support

- `strtotime()` only supports "YYYY-MM-DD" and "YYYY-MM-DD HH:MM:SS" formats. Relative time strings like "next Monday" or "+1 week" are not supported.
- `json_decode()` returns a string. It strips quotes and unescapes JSON string values, but does not parse JSON objects into associative arrays or JSON arrays into PHP arrays.
- `json_last_error()` always returns 0 — no actual error tracking is performed.
- `preg_*` functions ultimately run on POSIX extended regex via libc `regcomp`/`regexec`, but elephc first translates a small set of common PCRE shorthands such as `\s`, `\d`, `\w` and their uppercase negations. Lookahead, lookbehind, non-greedy quantifiers, and other PCRE-only features are still not supported.
- `preg_match()` does not support the `$matches` capture parameter.
- `preg_replace()` does not support backreferences like `$1` in the replacement string.

## Classes

elephc supports PHP classes with single inheritance, interfaces, abstract classes, properties, constructors, instance methods, static methods, traits, `self::method()`, `parent::method()`, `static::method()`, magic methods `__toString()` / `__get()` / `__set()`, and `public` / `protected` / `private` visibility.

### Class declaration

```php
<?php
class Shape {}

class Point extends Shape {
    public $x;
    public $y;

    public function __construct($x, $y) {
        $this->x = $x;
        $this->y = $y;
    }

    public function magnitude() {
        return sqrt($this->x * $this->x + $this->y * $this->y);
    }

    public static function origin() {
        return new Point(0, 0);
    }
}
```

Concrete classes may extend one parent class with `extends`. Method overrides use virtual dispatch, so an inherited method that calls `$this->otherMethod()` sees the child's override at runtime.

### Interfaces

Interfaces declare required public instance methods. They may extend multiple parent interfaces:

```php
<?php
interface Named {
    public function name();
}

interface Labeled extends Named {
    public function label();
}

class Product implements Labeled {
    public function name() {
        return "widget";
    }

    public function label() {
        return strtoupper($this->name());
    }
}
```

- Interface methods are signature-only: no method bodies, properties, or trait uses.
- Interface inheritance is flattened transitively with cycle detection.
- Concrete classes must implement every inherited interface method with a compatible parameter shape and `public` visibility.

### Abstract classes and abstract methods

Abstract classes can provide shared concrete behavior while deferring required methods to subclasses:

```php
<?php
abstract class BaseGreeter {
    abstract public function label();

    public function greet() {
        return "hi " . $this->label();
    }
}

class PersonGreeter extends BaseGreeter {
    public function label() {
        return "world";
    }
}
```

- `abstract class Name { ... }` cannot be instantiated with `new`.
- `abstract public function foo();` is supported inside abstract classes and traits.
- Non-abstract classes may not contain abstract methods.
- Abstract methods must be bodyless declarations ended by `;`.

### Properties

Properties are declared with a visibility modifier (`public`, `protected`, or `private`) and an optional default value:

```php
<?php
class Config {
    public $name = "default";
    public $debug = false;
    private $secret = "hidden";
    public readonly $id;

    public function __construct($id) {
        $this->id = $id;
    }
}
```

### Magic methods

elephc supports three PHP magic methods:

- `__toString()` is invoked when an object is coerced to string, including `echo $obj`, string concatenation, `(string)$obj`, and `settype($obj, "string")`
- `__get($name)` is invoked when reading an undefined property such as `$obj->title`
- `__set($name, $value)` is invoked when writing an undefined property such as `$obj->title = "hello"`

```php
<?php
class Post {
    public $log = "";

    public function __toString() {
        return "<post>";
    }

    public function __get($name) {
        return "[" . $name . "]";
    }

    public function __set($name, $value) {
        $this->log = $name . "=" . $value;
    }
}

$post = new Post();
$post->title = "Hello";
echo $post;         // <post>
echo $post->title;  // [title]
echo $post->log;    // title=Hello
```

Rules:

- `__toString()` must be `public`, non-static, take zero arguments, and return a string
- `__get()` must be `public`, non-static, and take exactly one argument
- `__set()` must be `public`, non-static, and take exactly two arguments
- If an object without `__toString()` is used in string context, elephc raises a runtime fatal error

- `public` properties can be accessed from outside the class via `->`.
- `protected` properties are not accessible from outside the class, but they are accessible inside subclasses.
- `private` properties can only be accessed inside the class via `$this->`.
- `readonly` properties can only be assigned inside `__construct`.
- Property redeclaration across an inheritance chain is not supported yet.

### Constructor

The `__construct` method is called automatically when creating a new object with `new`. Constructor parameters are passed as arguments to `new`:

```php
<?php
$p = new Point(3, 4);
```

### Instance methods and `$this`

Instance methods receive the object as `$this`. Use `$this->property` to access properties and `$this->method()` to call other methods:

```php
<?php
$p = new Point(3, 4);
echo $p->magnitude();  // 5
```

Overrides on subclasses use runtime dispatch:

```php
<?php
class Animal {
    public function speak() {
        return "animal";
    }

    public function run() {
        return $this->speak();
    }
}

class Dog extends Animal {
    public function speak() {
        return "dog";
    }
}

$dog = new Dog();
echo $dog->run();  // dog
```

Private methods are not virtual. If a parent method calls one of its own private helpers, that call stays bound to the parent's implementation even when the receiver is an instance of a child class.

### `parent::method()`

Inside a subclass method, `parent::method()` directly calls the parent implementation:

```php
<?php
class Base {
    public function greet() {
        return "hi";
    }
}

class Child extends Base {
    public function greet() {
        return parent::greet() . "!";
    }
}
```

### `self::method()`

Inside a class body, `self::method()` binds to the current lexical class rather than the runtime child override:

```php
<?php
class Base {
    public function reveal() {
        return self::label();
    }

    public function label() {
        return "base";
    }
}

class Child extends Base {
    public function label() {
        return "child";
    }
}

echo (new Child())->reveal(); // base
```

If `self::method()` resolves to an instance method, it is only allowed from a non-static method where `$this` exists.

### `static::method()`

`static::method()` uses late static binding: the method lookup happens against the current called class at runtime.

```php
<?php
class Base {
    public static function who() {
        return "base";
    }

    public static function relay() {
        return static::who();
    }
}

class Child extends Base {
    public static function who() {
        return "child";
    }
}

echo Child::relay(); // child
```

Forwarding rules match the current runtime model:

- `ClassName::method()` fixes the called class to `ClassName`
- `self::method()` resolves lexically but forwards the current called class
- `parent::method()` resolves to the immediate parent implementation and also forwards the current called class
- `static::method()` resolves dynamically against the forwarded called class

### Static methods

Static methods are called on the class itself using `::`, not on an instance:

```php
<?php
$origin = Point::origin();
echo $origin->x;  // 0
```

Static methods do not have access to `$this`.

Like instance methods, static methods honor `public`, `protected`, and `private` visibility. Inherited `public` and `protected` static methods remain callable through the child class; `private` static methods stay scoped to the declaring class.

### Override rules

For non-private inherited methods, elephc currently requires the child method to keep the same parameter shape as the parent:

- same parameter count
- same pass-by-reference positions
- same optional/default parameter layout
- same variadic vs non-variadic shape

### Traits

Traits are flattened into the concrete class at compile time. There is no runtime trait identity: imported members become ordinary class members before inheritance metadata and vtable slots are built.

```php
<?php
trait HasName {
    public $name = "elephc";

    public function target() {
        return $this->name;
    }
}

trait Greets {
    public function greet() {
        return "Hello";
    }
}

class Demo {
    use HasName, Greets {
        Greets::greet as baseGreet;
    }

    public function greetAll() {
        return $this->baseGreet() . ", " . $this->target();
    }
}

$demo = new Demo();
echo $demo->greetAll();
```

Supported trait features:

- `trait Name { ... }` declarations
- `use TraitName;` inside classes and traits
- multiple imported traits per `use`
- conflict resolution with `TraitA::method insteadof TraitB`
- aliasing and visibility remapping with `as`
- trait properties and static trait methods

Trait composition follows PHP-like precedence:

- Methods declared directly on the class win over imported trait methods.
- Conflicts between multiple traits must be resolved explicitly with `insteadof`.
- `as` creates an alias and/or changes visibility, but does not remove the original method by itself.

### Property access

Use `->` to access properties and call methods on objects:

```php
<?php
$p = new Point(3, 4);
echo $p->x;           // 3
$p->x = 10;           // assign property
echo $p->magnitude(); // method call
```

### Limitations

- No `final` classes or methods
- No property type declarations
- No constructor promotion
- Property redeclaration across an inheritance chain is rejected for now

## What elephc cannot do (by design)

- No enums
- No `eval()`
- No dynamic `include`/`require` (path must be a string literal)
- No generators/yield
- No fibers
- No attributes
- No named arguments
