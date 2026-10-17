---
title: "BCMath"
description: "Arbitrary-precision decimal arithmetic with PHP's 14 BCMath functions and immutable Number objects."
sidebar:
  order: 22
---

elephc implements PHP's 14 procedural `bcmath` functions and the immutable
`BcMath\Number` class through a pure-Rust
decimal bridge. Operations use base-10 digits directly, so values are not
converted through binary floating point and result strings preserve the requested
number of fractional digits.

## Functions

| Function | Signature | Result |
|---|---|---|
| `bcadd()` | `bcadd(string $num1, string $num2, ?int $scale = null): string` | Sum |
| `bcsub()` | `bcsub(string $num1, string $num2, ?int $scale = null): string` | Difference |
| `bcmul()` | `bcmul(string $num1, string $num2, ?int $scale = null): string` | Product |
| `bcdiv()` | `bcdiv(string $num1, string $num2, ?int $scale = null): string` | Quotient |
| `bcmod()` | `bcmod(string $num1, string $num2, ?int $scale = null): string` | Remainder |
| `bcdivmod()` | `bcdivmod(string $num1, string $num2, ?int $scale = null): array` | Indexed `[quotient, remainder]` pair |
| `bcpow()` | `bcpow(string $num, string $exponent, ?int $scale = null): string` | Integral power |
| `bcpowmod()` | `bcpowmod(string $num, string $exponent, string $modulus, ?int $scale = null): string` | Integral modular power |
| `bcsqrt()` | `bcsqrt(string $num, ?int $scale = null): string` | Square root |
| `bccomp()` | `bccomp(string $num1, string $num2, ?int $scale = null): int` | `-1`, `0`, or `1` |
| `bcscale()` | `bcscale(?int $scale = null): int` | Current scale, or previous scale when setting |
| `bcceil()` | `bcceil(string $num): string` | Least integer greater than or equal to the number |
| `bcfloor()` | `bcfloor(string $num): string` | Greatest integer less than or equal to the number |
| `bcround()` | `bcround(string $num, int $precision = 0, int $mode = 1): string` | Rounded decimal string |

The four PHP 8.4 additions, `bcceil()`, `bcfloor()`, `bcround()`, and
`bcdivmod()`, are available in every elephc PHP-version profile, like other
registry builtins.

## Scale and formatting

The process scale starts at `0`. Omitting `$scale`, or passing `null`, reads the
current value set by `bcscale()`; an explicit `0` always means zero fractional
digits. Setting the scale returns its previous value.

```php
echo bcscale(4);                 // 0
echo bcadd('1.234', '5');        // 6.2340
echo bcadd('1.234', '5', 0);     // 6
```

`bcadd()`, `bcsub()`, `bcmul()`, `bcdiv()`, and `bcmod()` truncate to the
selected scale; they do not round. Only `bcround()`, `bcceil()`, and
`bcfloor()` apply rounding. Output is normalized without a leading `+` or
negative zero, and is padded with trailing zeros to the selected scale.

`bcround()` accepts the same integer mode values as elephc's `round()`:
`1` through `8`, with `1` (`PHP_ROUND_HALF_UP` /
`RoundingMode::HalfAwayFromZero`) as the default.

## Accepted numbers and errors

Numeric strings are scanned verbatim and may contain an optional sign plus
decimal digits with an optional point. Forms such as `.5`, `5.`, and `+1.20`
are valid. PHP also normalizes digitless forms such as `""`, `+`, `-`, `.`,
`+.`, and `-.` to zero. Surrounding whitespace, exponent notation such as
`1e2`, multiple decimal points, and other non-numeric text are not accepted.

Malformed numbers, negative or out-of-range scales, invalid powers or square
roots, and unsupported rounding modes throw a catchable `ValueError`.
Division or modulo by zero, including a negative power of zero, throws a
catchable `DivisionByZeroError`. Error messages retain PHP function and
argument names.

## Linking and eval

Using a `bc*` function or `BcMath\Number` auto-links `libelephc_bcmath`.
Use `--with-bcmath` to force-link the bridge and retain the Number declarations
when the surface is reached only through indirection or opaque eval source:

```bash
elephc --with-bcmath app.php
```

`extension_loaded('bcmath')` reports `true` when the AOT binary links the
bridge. Dynamic `eval()` also exposes all 14 functions and shares the same
process scale with surrounding AOT code. Number methods, operators, and boolean
conversion also work on native Number objects inside eval. Opaque eval source
that creates Numbers without a visible native class reference requires
`--with-bcmath`.

## Immutable Number objects

`BcMath\Number` is a final readonly class implementing `Stringable`. Its
constructor accepts a decimal string or an integer. The public `$value` string
is normalized, and `$scale` counts its fractional digits, including trailing
zeroes. Each arithmetic result is a fresh Number; aliases retain the old value.

```php
use BcMath\Number;

$price = new Number('19.90');
$tax = ($price * '0.22')->round(2);
$total = $price + $tax;
echo $total->value;              // 24.28
echo $total->scale;              // 2
var_dump($total > 24);           // bool(true)
```

The methods `add()`, `sub()`, `mul()`, `div()`, `mod()`, `pow()`, and `compare()`
accept a Number, decimal string, or integer followed by an optional nullable
scale. `pow()` calls its operand `$exponent`. `powmod($exponent, $modulus,
?int $scale = null)` accepts the same operand types. `sqrt(?int $scale = null)`,
`ceil()`, and `floor()` return Numbers. `divmod()` returns an indexed pair of
Number objects: an integer quotient and a decimal remainder.

In coercive calls, constructor and method operands follow PHP's `string|int`
binding: finite floats within the signed 64-bit integer range truncate towards
zero, booleans become `0` or `1`, and null becomes `0`. For example,
`(new Number('2.50'))->add(1.5)->value` is `"3.50"`. Nonfinite and out-of-range
floats fail decimal validation with `ValueError`. Arithmetic operators truncate
in-range floats too, but reject nonfinite and out-of-range floats with `TypeError`.

`round(int $precision = 0, RoundingMode $mode =
RoundingMode::HalfAwayFromZero)` supports all eight PHP rounding modes.
`__toString()`, `__serialize()`, and `__unserialize()` preserve the decimal
value and its full scale. Serialized data contains the `value` string;
unserialization derives the scale from that string.

An omitted or null method scale is independent of `bcscale()`:

| Operation | Automatic scale |
|---|---|
| Addition, subtraction, modulo | Maximum operand scale |
| Multiplication | Sum of operand scales |
| Division, square root, negative power | Receiver scale plus up to ten extra digits, removing unnecessary expansion zeroes |
| Nonnegative power | Receiver scale multiplied by the exponent; zero for exponent zero |
| Modular power | Zero |
| Comparison | Full operand precision |
| Divmod | Integer quotient; remainder uses the maximum operand scale |

An explicit scale truncates and pads to exactly that many fractional digits.
The arithmetic operators `+`, `-`, `*`, `/`, `%`, and `**` use automatic scale,
as do unary `+`/`-`, compound assignments, and immutable `++`/`--` replacements.
Value comparisons `==`, `!=`, `<`, `<=`, `>`, `>=`, and `<=>` compare decimals.
Strict `===` and `!==` compare object identity. A zero Number is false in
boolean contexts, including `empty()`.
`switch` cases and non-strict `in_array()` use the same decimal equality, so
`1.20` and `1.200` match despite their different scales. Strict membership keeps
object identity. Boolean comparisons follow PHP 8.5 truthiness: a nonzero Number
compares equal to `true`, and a zero Number compares equal to `false`.

## Current scope

The PHP symbol surface includes all 14 procedural functions and the Number
class. The `bcmath.scale` INI directive remains unsupported; `bcscale()` is the
supported process-scale interface. Procedural `bcround()` retains its integer
mode interface described above; Number's `round()` uses PHP's `RoundingMode` enum.

<!-- elephc:generated:symbols:begin -->

## Functions {#functions}

Generated from the shared symbol catalog by `scripts/docs/gen_module_sections.py`; do not edit this section by hand. Each function links to its reference page.

| Function | Signature | Returns | AOT | eval() |
|---|---|---|:-:|:-:|
| [`bcadd()`](./builtins/math/bcadd.md) | `(string $num1, string $num2, ?int $scale = null): string` | `string` | ✓ | ✓ |
| [`bcceil()`](./builtins/math/bcceil.md) | `(string $num): string` | `string` | ✓ | ✓ |
| [`bccomp()`](./builtins/math/bccomp.md) | `(string $num1, string $num2, ?int $scale = null): int` | `int` | ✓ | ✓ |
| [`bcdiv()`](./builtins/math/bcdiv.md) | `(string $num1, string $num2, ?int $scale = null): string` | `string` | ✓ | ✓ |
| [`bcdivmod()`](./builtins/math/bcdivmod.md) | `(string $num1, string $num2, ?int $scale = null): array` | `array` | ✓ | ✓ |
| [`bcfloor()`](./builtins/math/bcfloor.md) | `(string $num): string` | `string` | ✓ | ✓ |
| [`bcmod()`](./builtins/math/bcmod.md) | `(string $num1, string $num2, ?int $scale = null): string` | `string` | ✓ | ✓ |
| [`bcmul()`](./builtins/math/bcmul.md) | `(string $num1, string $num2, ?int $scale = null): string` | `string` | ✓ | ✓ |
| [`bcpow()`](./builtins/math/bcpow.md) | `(string $num, string $exponent, ?int $scale = null): string` | `string` | ✓ | ✓ |
| [`bcpowmod()`](./builtins/math/bcpowmod.md) | `(string $num, string $exponent, string $modulus, ?int $scale = null): string` | `string` | ✓ | ✓ |
| [`bcround()`](./builtins/math/bcround.md) | `(string $num, int $precision = 0, int $mode = 1): string` | `string` | ✓ | ✓ |
| [`bcscale()`](./builtins/math/bcscale.md) | `(?int $scale = null): int` | `int` | ✓ | ✓ |
| [`bcsqrt()`](./builtins/math/bcsqrt.md) | `(string $num, ?int $scale = null): string` | `string` | ✓ | ✓ |
| [`bcsub()`](./builtins/math/bcsub.md) | `(string $num1, string $num2, ?int $scale = null): string` | `string` | ✓ | ✓ |

Classes: `BcMath\Number`.

<!-- elephc:generated:symbols:end -->
