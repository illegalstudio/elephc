---
title: "GMP"
description: "Arbitrary-precision integers with php-src's ext/gmp surface over a pinned libgmp 6.3.0, including operators, casts, serialization and all 51 functions."
sidebar:
  order: 22
---

elephc implements PHP's **GMP** extension (arbitrary-precision integers) by
linking the same library php-src uses: a pinned **GMP 6.3.0** static archive
managed by the native-package catalog. The PHP-visible surface is the complete
php-src 8.5 one — the final `GMP` class, every procedural function, the operator
and cast handlers, and both serialization formats.

The extension is pay-for-use. A program that never names a GMP function or the
`GMP` class does not inject the prelude, does not link `libgmp`, and does not
require the managed package.

## Enable GMP

Detection is automatic when the program calls a `gmp_*` function, names the
`GMP` class, or declares a `GMP` parameter or property. Install the managed
package once per target:

```bash
elephc native add gmp
```

Force the whole surface for dynamic code whose calls are not visible in the
source (an interpreted string, a serialized class name, or a callable table
behind `mixed`):

```bash
elephc --with-gmp program.php
```

A missing package fails closed with the `elephc native add gmp` recovery; there
is no system `-lgmp` fallback. The iOS targets link the same static archive and
need no extra system framework.

## Inside `eval()`

The same compiled GMP prelude backs `eval()` fragments. Because the fragment
source is only known at run time, force the surface when it is the only caller:

```bash
elephc --with-gmp program.php
```

`eval()` supports the whole procedural surface, `new GMP()`, operators,
comparisons, `(int)`/`(float)`/`(bool)`/`(string)` casts, named arguments,
first-class callables, `sort()`/`min()`/`max()`/`array_sum()`/`pow()`,
`extension_loaded('gmp')`, and the `GMP_*` constants. A program that did not
inject the prelude has no helper registered, so a GMP call from `eval()` raises
PHP's catchable `Error: Call to undefined function gmp_add()` instead of
running a partial surface.

`serialize()` and `unserialize()` use the compiled boxed-cell runtime from
`eval()` too, including GMP's `O:` form, the legacy `C:` form and dynamic
properties. Other eval-only language gaps that affect a few php-src fixtures
but not GMP semantics are `(array)` casts, `var_export()`, `++`/`--`, `break N`,
arrow functions, and assignment expressions inside call arguments (the last
one now works). Error messages that name an operand declared by `eval()` itself
resolve the owning eval context, so they report the declared class name instead
of the native `stdClass` carrier.

`method_exists()`, `get_class_methods()` and `ReflectionClass::getMethods()`
expose only php-src's three GMP methods (`__construct`, `__serialize`,
`__unserialize`); the compiler's operator/cast adapters remain internal to the
class implementation.

## The `GMP` class

`GMP` is a `final` class. It is constructed directly or through `gmp_init()`:

```php
$n = new GMP('123456789012345678901234567890');
$m = gmp_init('ffffffff', 16);
var_dump($n > $m);
```

The constructor follows php-src's rules: the first argument is `string|int`,
the optional base is `0` or `2`–`62`, an explicit prefix (`0x`, `0o`, `0b`) is
accepted only for its matching base, and a prefix with no digits
(`'0x'`, `'0b'`, `'0o'`) is a `ValueError`.

`GMP` values take part in PHP's ordinary operators:

| Operator | Result |
|---|---|
| `+`, `-`, `*`, `/`, `%` | exact integer result; division truncates toward zero like php-src |
| `**` | non-negative integer exponent |
| `<<`, `>>` | shift by a non-negative `int` |
| `&`, `\|`, `^`, `~` | infinite two's-complement bitwise result |
| `-` (unary) | negation |
| `<`, `<=`, `>`, `>=`, `==`, `<=>`, `!=` | numeric ordering, with GMP coercing the other operand |
| `(int)`, `(float)`, `(bool)`, `(string)` | php-src's cast handlers |

Arithmetic and comparison operands accept `GMP|string|int` after PHP's weak
coercion, and every unsupported family (float outside `int` range, arrays,
`null`, resources, foreign objects) raises the same `TypeError` wording as
php-src. Division and modulo by zero raise `DivisionByZeroError`.

`gmp_invert()` returns `GMP|false`; a modulus with no inverse yields `false`
rather than an exception.

## Procedural functions

All 51 php-src functions are available under their PHP names:

- **Construction and conversion** — `gmp_init`, `gmp_intval`, `gmp_strval`,
  `gmp_export`, `gmp_import`.
- **Arithmetic** — `gmp_add`, `gmp_sub`, `gmp_mul`, `gmp_div`,
  `gmp_div_q`, `gmp_div_r`, `gmp_div_qr`, `gmp_divexact`, `gmp_mod`,
  `gmp_pow`, `gmp_powm`, `gmp_neg`, `gmp_abs`.
- **Number theory** — `gmp_fact`, `gmp_binomial`, `gmp_gcd`, `gmp_gcdext`,
  `gmp_lcm`, `gmp_invert`, `gmp_jacobi`, `gmp_legendre`, `gmp_kronecker`,
  `gmp_nextprime`, `gmp_prob_prime`, `gmp_perfect_square`,
  `gmp_perfect_power`, `gmp_sqrt`, `gmp_sqrtrem`, `gmp_root`, `gmp_rootrem`.
- **Bits** — `gmp_and`, `gmp_or`, `gmp_xor`, `gmp_com`, `gmp_popcount`,
  `gmp_hamdist`, `gmp_testbit`, `gmp_setbit`, `gmp_clrbit`, `gmp_scan0`,
  `gmp_scan1`, `gmp_sign`, `gmp_cmp`.
- **Randomness** — `gmp_random_seed`, `gmp_random_bits`, `gmp_random_range`.

The division-mode constants `GMP_ROUND_ZERO`, `GMP_ROUND_PLUSINF` and
`GMP_ROUND_MINUSINF`, the import/export flags (`GMP_MSW_FIRST`,
`GMP_LSW_FIRST`, `GMP_LITTLE_ENDIAN`, `GMP_BIG_ENDIAN`,
`GMP_NATIVE_ENDIAN`), and `GMP_VERSION` are defined. `GMP_VERSION` reports the
pinned library version, `6.3.0`.

`gmp_export()` / `gmp_import()` honour the word size, word order and byte order
flags exactly, including php-src's "word size too large" and
"data length not a multiple of word size" diagnostics.

`gmp_setbit()` and `gmp_clrbit()` mutate their first argument in place, as in
php-src, and require a real `GMP` object.

### Random state

`gmp_random_seed()` seeds the process-global MT generator; `gmp_random_bits()`
and `gmp_random_range()` draw from that same stream. The bridge reuses libgmp's
generator, so a fixed seed reproduces php-src's exact sequence.

## Serialization

`serialize()` writes PHP 8's object form, with the value stored as hexadecimal:

```php
$g = gmp_init(42);
echo serialize($g); // O:3:"GMP":1:{i:0;s:2:"2a";}
```

`unserialize()` accepts that form and the legacy `C:3:"GMP":...` payload emitted
by PHP 5 and early PHP 7. A malformed legacy payload raises php-src's
`Exception: Could not unserialize number`, and malformed wire data returns
`false` after the usual `Warning: unserialize(): Error at offset N of M bytes`.

The serialized property table may contain dynamic properties. They are restored
on the object and, like any other write to a `GMP` instance, report PHP 8.2's
`Creation of dynamic property` deprecation.

## Runtime integration

GMP comparisons cooperate with PHP's runtime container helpers:

- `sort()`, `rsort()` and the mixed-slot sorter order `GMP` values with the same
  comparison handler used by `<` and `<=>`; unhandled object classes still fail
  closed instead of sorting as equal.
- `min()` / `max()` and single-array `min()`/`max()` see GMP's numeric ordering.
- `array_sum()` converts a `GMP` value through its `_IS_NUMBER` handler, so a
  value that fits becomes an `int` and a larger one a `float`, as php-src does.
- `pow()` and `min()`/`max()` accept GMP operands through the same operator
  adapters as `**` and the relational operators.

## Example

```php
<?php
$factorial = gmp_fact(30);
echo gmp_strval($factorial), "\n";

$modulus = gmp_init('fffffffffffffffffffffffffffffffeffffffffffffffff', 16);
$power = gmp_powm(gmp_init('deadbeefcafebabe', 16), 65537, $modulus);
echo gmp_strval($power, 16), "\n";
```

A complete runnable example lives in `examples/gmp/main.php`.

Very large values are bounded by the compiler's fixed heap. The default is
8 MiB; a workload such as `1000!` needs php-src's usual 128 MiB headroom, so
compile it with `--heap-size=134217728` (or larger).

## Compatibility notes

- The pinned library is **GMP 6.3.0**, built from the signed upstream tarball by
  the managed recipe. `GMP_VERSION` reflects that pin.
- Values cross the bridge as decimal strings, so results are exactly the ones
  libgmp produces; there is no Rust bigint reimplementation.
- `GMP`'s compiler storage property is an implementation detail. It is omitted
  from `get_object_vars()`, `(array)` casts, JSON encoding and serialization;
  the PHP-visible debug projection is the virtual `num` property, matching
  php-src's `var_dump()` shape.
