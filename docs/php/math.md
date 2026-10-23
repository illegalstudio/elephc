---
title: "Math"
description: "Mathematical functions: abs, floor, ceil, round, clamp, trigonometry, logarithms, and more."
sidebar:
  order: 9
---

## Built-in math functions

| Function | Signature | Description |
|---|---|---|
| `abs()` | `abs($val): int\|float` | Absolute value (preserves type); `abs(PHP_INT_MIN)` has no `int` result and promotes to `float` |
| `floor()` | `floor($val): float` | Round down |
| `ceil()` | `ceil($val): float` | Round up |
| `round()` | `round($num [, $precision [, $mode]]): float` | Round to nearest. `$mode` is one of `PHP_ROUND_HALF_UP` (default), `PHP_ROUND_HALF_DOWN`, `PHP_ROUND_HALF_EVEN`, `PHP_ROUND_HALF_ODD`; any other value throws `\ValueError` |
| `sqrt()` | `sqrt($val): float` | Square root |
| `pow()` | `pow($base, $exp): float` | Exponentiation |
| `min()` | `min($value, ...$values): mixed` | Minimum. Either one array, or two or more values |
| `max()` | `max($value, ...$values): mixed` | Maximum. Either one array, or two or more values |
| `clamp()` | `clamp(?mixed $value, ?mixed $min, ?mixed $max): ?mixed` | Clamp a value to inclusive bounds |
| `intdiv()` | `intdiv($a, $b): int` | Integer division; a zero divisor raises a catchable `DivisionByZeroError`, and `intdiv(PHP_INT_MIN, -1)` an `ArithmeticError` |
| `fmod()` | `fmod($a, $b): float` | Float modulo |
| `fdiv()` | `fdiv($a, $b): float` | Float division (returns INF for /0) |
| `rand()` | `rand([$min, $max]): int` | Random integer |
| `mt_rand()` | `mt_rand([$min, $max]): int` | Alias for rand(), except that `$min > $max` throws `\ValueError` instead of swapping the bounds |
| `random_int()` | `random_int($min, $max): int` | Cryptographic random. `$min > $max` throws `\ValueError`. |
| `random_bytes()` | `random_bytes($length): string` | Cryptographically secure random bytes from the operating system (`arc4random_buf` on macOS, `getrandom` on Linux). A `$length` below 1 throws `\ValueError`; if the system refuses to provide randomness an `\Exception` is thrown (`Random\RandomException` is not declared yet). |
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
| `log()` | `log($num [, $base]): float` | Logarithm |
| `log2()` | `log2($num): float` | Base-2 logarithm |
| `log10()` | `log10($num): float` | Base-10 logarithm |
| `exp()` | `exp($val): float` | e^x |
| `hypot()` | `hypot($x, $y): float` | Hypotenuse |
| `deg2rad()` | `deg2rad($degrees): float` | Degrees to radians |
| `rad2deg()` | `rad2deg($radians): float` | Radians to degrees |
| `pi()` | `pi(): float` | Returns M_PI |
| `base_convert()` | `base_convert($num, $from_base, $to_base): string` | Re-render a numeral between two bases from 2 to 36. Letter digits are case-insensitive and characters that are not digits of `$from_base` are ignored. A base outside 2-36 throws `\ValueError`. |

`clamp()` validates the bounds before selecting a result. It throws `ValueError` if `$min > $max` or if either bound is `NAN`. Selection checks the upper bound first, then the lower bound.

```php
echo clamp(15, 0, 10);      // 10
echo clamp(3.5, 0.0, 10.0); // 3.5
echo clamp("P", "A", "Z");  // "P"
```

### base_convert()

`base_convert()` parses `$num` in `$from_base` and re-renders it in `$to_base`. Digits above
`9` use the letters `a`-`z` in either case, and any character that is not a digit of
`$from_base` is skipped rather than ending the scan.

```php
echo base_convert("ff", 16, 10);      // 255
echo base_convert("a37334", 16, 2);   // 101000110111001100110100
echo base_convert("zz", 36, 10);      // 1295
```

A value larger than `PHP_INT_MAX` widens to a float during the parse, exactly as in reference
PHP, and the rendered digits are then rounded rather than exact — `base_convert("ffffffffffffffff", 16, 10)`
is `"18446744073709552046"`, not `"18446744073709551615"`. The float render also stops after 64
digits.

For exact arbitrary-precision decimal strings, including process-wide scale,
truncating arithmetic, decimal rounding, powers, square roots, and quotient/remainder
pairs, see [BCMath](bcmath.md).

### min() and max()

Both accept PHP's two call forms: a single array whose elements are compared, or
two or more values compared against each other.

```php
var_dump(min([1, 2, 3]));   // int(1)
var_dump(max([1, 2, 3]));   // int(3)
var_dump(min([3.5, 1.25])); // float(1.25)
var_dump(min(4, 9, 2));     // int(2)
```

An empty array has no element to return, so it throws a catchable `ValueError`
exactly like PHP:

```php
try {
    min([]);
} catch (ValueError $e) {
    echo $e->getMessage(); // min(): Argument #1 ($value) must contain at least one element
}
```

The single-array form accepts indexed arrays of `int`, `float`, `bool` and `string`,
indexed arrays with heterogeneous (boxed `mixed`) elements, and associative arrays with
values of any type:

```php
var_dump(min([1, 2.5]));                     // int(1)
var_dump(max(["a", "c", "b"]));              // string(1) "c"
var_dump(min(["a" => 3, "b" => 1]));         // int(1)
var_dump(max(["x" => "pear", "y" => "fig"])); // string(4) "pear"
```

Elements are compared with PHP 8's own comparison rules, in PHP's order: a `bool` on
either side converts both sides to `bool`, then `null` (which becomes `""` against a
string, so `min([null, "a"])` is `NULL` but `min(["", null])` is `""`), then two numeric
strings compare numerically while any other string pair compares byte-wise, and finally a
number against a non-numeric string compares as strings. Ties keep the *earlier* element
and the winner keeps its original type, exactly like PHP:

```php
var_dump(min(["10", "9"]));   // string(1) "9"  — both numeric, so 9 < 10
var_dump(min(["10", "9a"]));  // string(2) "10" — "9a" is not numeric, so bytes decide
var_dump(max([0, "a"]));      // string(1) "a"  — "0" vs "a" as strings
var_dump(min([1, "1"]));      // int(1)         — equal, so the first element wins
```

Two limitations remain in the single-array form:

- Comparisons that involve a *numeric string* are resolved as `double`s, so two integer
  strings that differ only beyond 2^53 (`min(["9223372036854775807", "9223372036854775806"])`)
  can compare equal where PHP compares them exactly. Comparisons between two real `int`
  elements are exact. This is the same simplification the `==` runtime already makes.
- Arrays, objects, resources and callables only rank *above* the scalar elements and
  compare equal to each other, instead of PHP's element-wise array comparison. An indexed
  array whose elements are themselves arrays (`min([[1], [2]])`) is rejected at compile
  time rather than reduced with the wrong order.

## Math constants

| Constant | Type | Value |
|---|---|---|
| `M_PI` | float | 3.14159265358979... |
| `M_E` | float | 2.71828182845904... |
| `M_SQRT2` | float | 1.41421356237309... |
| `M_PI_2` | float | 1.57079632679489... |
| `M_PI_4` | float | 0.78539816339744... |
| `M_LOG2E` | float | 1.44269504088896... |
| `M_LOG10E` | float | 0.43429448190325... |
| `INF` | float | Positive infinity |
| `NAN` | float | Not a Number |
| `PHP_INT_MAX` | int | 9223372036854775807 |
| `PHP_INT_MIN` | int | -9223372036854775808 |
| `PHP_FLOAT_MAX` | float | ~1.8e308 |
| `PHP_FLOAT_MIN` | float | ~2.2e-308 |
| `PHP_FLOAT_EPSILON` | float | ~2.2e-16 |
