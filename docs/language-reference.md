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

**Not supported yet:** `<=>` (spaceship).

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
| `floatval()` | `floatval($val): float` | Convert to float |
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
| `gettype()` | `gettype($val): string` | Returns type name ("integer", "double", "string", "boolean", "NULL", "array") |
| `empty()` | `empty($val): bool` | Returns true if value is falsy (0, 0.0, "", false, null, empty array) |
| `unset()` | `unset($var): void` | Sets variable to null |
| `settype()` | `settype($var, $type): bool` | Changes variable type in place |

### System functions

| Function | Signature | Description |
|---|---|---|
| `exit()` | `exit($code = 0): void` | Terminate program |
| `die()` | `die(): void` | Alias for `exit(0)` |

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

| Constant | Type | Value |
|---|---|---|
| `INF` | float | Positive infinity |
| `NAN` | float | Not a Number |
| `PHP_INT_MAX` | int | 9223372036854775807 |
| `PHP_INT_MIN` | int | -9223372036854775808 |
| `PHP_FLOAT_MAX` | float | ~1.8e308 |
| `M_PI` | float | 3.14159265358979... |
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
