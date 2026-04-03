---
title: "Math"
description: "Mathematical functions: abs, floor, ceil, round, trigonometry, logarithms, and more."
sidebar:
  order: 7
---

## Built-in math functions

| Function | Signature | Description |
|---|---|---|
| `abs()` | `abs($val): int\|float` | Absolute value (preserves type) |
| `floor()` | `floor($val): float` | Round down |
| `ceil()` | `ceil($val): float` | Round up |
| `round()` | `round($val [, $precision]): float` | Round to nearest |
| `sqrt()` | `sqrt($val): float` | Square root |
| `pow()` | `pow($base, $exp): float` | Exponentiation |
| `min()` | `min($a, $b, ...): int\|float` | Minimum (variadic) |
| `max()` | `max($a, $b, ...): int\|float` | Maximum (variadic) |
| `intdiv()` | `intdiv($a, $b): int` | Integer division |
| `fmod()` | `fmod($a, $b): float` | Float modulo |
| `fdiv()` | `fdiv($a, $b): float` | Float division (returns INF for /0) |
| `rand()` | `rand([$min, $max]): int` | Random integer |
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
| `log()` | `log($num [, $base]): float` | Logarithm |
| `log2()` | `log2($num): float` | Base-2 logarithm |
| `log10()` | `log10($num): float` | Base-10 logarithm |
| `exp()` | `exp($val): float` | e^x |
| `hypot()` | `hypot($x, $y): float` | Hypotenuse |
| `deg2rad()` | `deg2rad($degrees): float` | Degrees to radians |
| `rad2deg()` | `rad2deg($radians): float` | Radians to degrees |
| `pi()` | `pi(): float` | Returns M_PI |

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
