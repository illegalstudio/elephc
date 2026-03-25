# elephc Language Reference

This document describes the PHP subset supported by elephc. Every program listed here is valid PHP and produces identical output when run with `php`.

## Data Types

| Type | Supported | Notes |
|---|---|---|
| `int` | Yes | 64-bit signed integer |
| `string` | Yes | Pointer + length pair, double and single quoted |
| `null` | Yes | Sentinel value, coerces to `0`/`""` in operations |
| `bool` | Yes | `true`/`false` as distinct type. `echo false` prints nothing, `echo true` prints `1`. Coerces to 0/1 in arithmetic. |
| `float` | Yes | 64-bit double-precision. Literals: `3.14`, `.5`, `1.5e3`, `1.0e-5`. Constants: `INF`, `NAN`. |
| `array` | Yes | Indexed (`[1, 2, 3]`) and associative (`["key" => "value"]`). Hash table runtime for string keys. |
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

- `$argv[0]` returns the compiled binary path, not the `.php` file path.

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
| `==` | `$a == $b` | Loose equality (integers only) |
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

**Limitation:** `use ($var)` captures are not yet supported. Closures cannot access variables from the enclosing scope.

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

**Limitation:** Like closures, arrow functions do not yet capture variables from the enclosing scope.

### Limitations

- No pass by reference (`function foo(&$x)` — not supported)
- No variadic functions (`function foo(...$args)` — not supported)
- No `use ($var)` captures in closures
- No `global` keyword

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

Associative arrays use a hash table runtime for string keys. Keys are always strings; values must all be the same type.

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
- Arrays are homogeneous: all elements must be the same type

## Built-in Functions

### String functions

| Function | Signature | Description |
|---|---|---|
| `strlen()` | `strlen($str): int` | Returns string length |
| `number_format()` | `number_format($n [, $dec [, $dec_point, $thou_sep]]): string` | Format number with separators |
| `substr()` | `substr($str, $start [, $len]): string` | Extract substring |
| `strpos()` | `strpos($hay, $needle): int` | Find first occurrence (-1 if not found) |
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
| `trim()` | `trim($str): string` | Strip whitespace from both ends |
| `ltrim()` | `ltrim($str): string` | Strip whitespace from left |
| `rtrim()` | `rtrim($str): string` | Strip whitespace from right |
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
| `wordwrap()` | `wordwrap($str [, $width, $break]): string` | Wrap text at width |
| `bin2hex()` | `bin2hex($str): string` | Convert binary to hex |
| `hex2bin()` | `hex2bin($str): string` | Convert hex to binary |
| `sprintf()` | `sprintf($fmt, ...): string` | Format string (%s, %d, %f, %x, %%) |
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
| `array_push()` | `array_push($arr, $val): void` | Add element to end |
| `array_pop()` | `array_pop($arr): mixed` | Remove and return last element |
| `in_array()` | `in_array($needle, $arr): int` | Search for value (returns 0/1) |
| `array_keys()` | `array_keys($arr): array` | Returns [0, 1, 2, ...] |
| `array_values()` | `array_values($arr): array` | Returns copy of values |
| `sort()` | `sort($arr): void` | Sort ascending (in-place) |
| `rsort()` | `rsort($arr): void` | Sort descending (in-place) |
| `isset()` | `isset($var): int` | Check if variable is defined (always 1) |
| **Searching** | | |
| `array_key_exists()` | `array_key_exists($key, $arr): bool` | Check if key exists in array |
| `array_search()` | `array_search($needle, $arr): int\|string\|bool` | Search for value, return key or false |
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
| `array_rand()` | `array_rand($arr [, $num]): int\|array` | Pick random key(s) |
| **Multi-dimensional** | | |
| `array_column()` | `array_column($arr, $column_key): array` | Extract column from array of assoc arrays |
| **Callback-based** | | |
| `array_map()` | `array_map("callback", $arr): array` | Apply callback to each element, return new array |
| `array_filter()` | `array_filter($arr, "callback"): array` | Filter elements where callback returns truthy |
| `array_reduce()` | `array_reduce($arr, "callback", $init): mixed` | Reduce array to single value via callback |
| `array_walk()` | `array_walk($arr, "callback"): void` | Call callback on each element (side-effects) |
| `usort()` | `usort($arr, "callback"): void` | Sort array using user comparison function |
| `uksort()` | `uksort($arr, "callback"): void` | Sort by keys using user comparison function |
| `uasort()` | `uasort($arr, "callback"): void` | Sort by values using user comparison, maintain keys |
| **Function handling** | | |
| `call_user_func()` | `call_user_func("name", ...): mixed` | Call a function by name with arguments |
| `call_user_func_array()` | `call_user_func_array("name", $args): mixed` | Call a function with arguments from an array |
| `function_exists()` | `function_exists("name"): bool` | Check if a function is defined |
| `define()` | `define("NAME", value): void` | Define a named constant |

> **Note:** Callback arguments can be string literals containing the function name (e.g., `"double"`), anonymous functions, or arrow functions.

**Not yet supported:** `compact()`, `extract()` (require dynamic variables).

### Math functions

| Function | Signature | Description |
|---|---|---|
| `abs()` | `abs($val): int\|float` | Absolute value (preserves type) |
| `floor()` | `floor($val): float` | Round down |
| `ceil()` | `ceil($val): float` | Round up |
| `round()` | `round($val): float` | Round to nearest |
| `sqrt()` | `sqrt($val): float` | Square root |
| `pow()` | `pow($base, $exp): float` | Exponentiation |
| `min()` | `min($a, $b): int\|float` | Minimum of two values |
| `max()` | `max($a, $b): int\|float` | Maximum of two values |
| `intdiv()` | `intdiv($a, $b): int` | Integer division |
| `fmod()` | `fmod($a, $b): float` | Float modulo |
| `fdiv()` | `fdiv($a, $b): float` | Float division (returns INF for /0) |
| `rand()` | `rand([$min, $max]): int` | Random integer (0 or 2 args) |
| `mt_rand()` | `mt_rand([$min, $max]): int` | Alias for rand() |
| `random_int()` | `random_int($min, $max): int` | Cryptographic random |

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
| `gettype()` | `gettype($val): string` | Returns type name ("integer", "double", "string", "boolean", "NULL", "array") |
| `empty()` | `empty($val): bool` | Returns true if value is falsy (0, 0.0, "", false, null, empty array) |
| `unset()` | `unset($var): void` | Sets variable to null |
| `settype()` | `settype($var, $type): bool` | Changes variable type in place |

### System functions

| Function | Signature | Description |
|---|---|---|
| `exit()` | `exit($code = 0): void` | Terminate program |
| `die()` | `die(): void` | Alias for `exit(0)` |
| `time()` | `time(): int` | Get current Unix timestamp |
| `microtime()` | `microtime($as_float = false): float` | Get current time with microsecond precision |
| `sleep()` | `sleep($seconds): int` | Sleep for given seconds |
| `usleep()` | `usleep($microseconds): void` | Sleep for given microseconds |
| `getenv()` | `getenv($name): string` | Get environment variable value |
| `putenv()` | `putenv($assignment): bool` | Set environment variable (format: "KEY=VALUE") |
| `php_uname()` | `php_uname(): string` | Get OS name (returns "Darwin") |
| `phpversion()` | `phpversion(): string` | Get elephc version string |
| `exec()` | `exec($command): string` | Execute command, return output |
| `shell_exec()` | `shell_exec($command): string` | Execute command via shell, return full output |
| `system()` | `system($command): string` | Execute command, output to stdout |
| `passthru()` | `passthru($command): void` | Execute command, pass raw output to stdout |

### I/O functions

| Function | Signature | Description |
|---|---|---|
| `fopen()` | `fopen($filename, $mode): resource` | Open a file (modes: r, w, a, r+, w+, a+) |
| `fclose()` | `fclose($handle): bool` | Close a file handle |
| `fread()` | `fread($handle, $length): string` | Read up to $length bytes |
| `fwrite()` | `fwrite($handle, $data): int` | Write string to file, returns bytes written |
| `fgets()` | `fgets($handle): string` | Read a line from file or STDIN |
| `feof()` | `feof($handle): bool` | Check if end-of-file reached |
| `readline()` | `readline([$prompt]): string` | Read a line from STDIN |
| `fseek()` | `fseek($handle, $offset): int` | Seek to position in file |
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
| `M_PI` | float | 3.14159265358979... |
| `PHP_EOL` | string | End of line character (`"\n"`) |
| `PHP_OS` | string | Operating system name (`"Darwin"`) |
| `DIRECTORY_SEPARATOR` | string | Directory separator (`"/"`) |
| `STDIN` | resource | Standard input stream (fd 0) |
| `STDOUT` | resource | Standard output stream (fd 1) |
| `STDERR` | resource | Standard error stream (fd 2) |

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

## What elephc cannot do (by design)

- No classes, objects, interfaces, traits, enums
- No exceptions (`try`/`catch`/`throw`)
- No `eval()`
- No dynamic `include`/`require` (path must be a string literal)
- No namespaces
- No generators/yield
- No fibers
- No attributes
- No named arguments
