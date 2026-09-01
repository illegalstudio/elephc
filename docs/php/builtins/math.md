---
title: "Math builtins"
description: "Builtins in the Math category."
sidebar:
  order: 103
---

## Math builtins

| Function | Signature | Returns | AOT | eval() |
|---|---|---|:-:|:-:|
| [`abs()`](./math/abs.md) | `(int $num): mixed` | `mixed` | ✓ | ✓ |
| [`acos()`](./math/acos.md) | `(float $num): float` | `float` | ✓ | ✓ |
| [`asin()`](./math/asin.md) | `(float $num): float` | `float` | ✓ | ✓ |
| [`atan()`](./math/atan.md) | `(float $num): float` | `float` | ✓ | ✓ |
| [`atan2()`](./math/atan2.md) | `(float $y, float $x): float` | `float` | ✓ | ✓ |
| [`base_convert()`](./math/base_convert.md) | `(string $num, int $from_base, int $to_base): string` | `string` | ✓ | ✓ |
| [`bcadd()`](./math/bcadd.md) | `(string $num1, string $num2, int $scale = null): string` | `string` | ✓ | ✓ |
| [`bcceil()`](./math/bcceil.md) | `(string $num): string` | `string` | ✓ | ✓ |
| [`bccomp()`](./math/bccomp.md) | `(string $num1, string $num2, int $scale = null): int` | `int` | ✓ | ✓ |
| [`bcdiv()`](./math/bcdiv.md) | `(string $num1, string $num2, int $scale = null): string` | `string` | ✓ | ✓ |
| [`bcdivmod()`](./math/bcdivmod.md) | `(string $num1, string $num2, int $scale = null): array` | `array` | ✓ | ✓ |
| [`bcfloor()`](./math/bcfloor.md) | `(string $num): string` | `string` | ✓ | ✓ |
| [`bcmod()`](./math/bcmod.md) | `(string $num1, string $num2, int $scale = null): string` | `string` | ✓ | ✓ |
| [`bcmul()`](./math/bcmul.md) | `(string $num1, string $num2, int $scale = null): string` | `string` | ✓ | ✓ |
| [`bcpow()`](./math/bcpow.md) | `(string $num, string $exponent, int $scale = null): string` | `string` | ✓ | ✓ |
| [`bcpowmod()`](./math/bcpowmod.md) | `(string $num, string $exponent, string $modulus, int $scale = null): string` | `string` | ✓ | ✓ |
| [`bcround()`](./math/bcround.md) | `(string $num, int $precision = 0, int $mode = 1): string` | `string` | ✓ | ✓ |
| [`bcscale()`](./math/bcscale.md) | `(int $scale = null): int` | `int` | ✓ | ✓ |
| [`bcsqrt()`](./math/bcsqrt.md) | `(string $num, int $scale = null): string` | `string` | ✓ | ✓ |
| [`bcsub()`](./math/bcsub.md) | `(string $num1, string $num2, int $scale = null): string` | `string` | ✓ | ✓ |
| [`bindec()`](./math/bindec.md) | `(string $binary_string): mixed` | `mixed` | ✓ | — |
| [`ceil()`](./math/ceil.md) | `(float $num): float` | `float` | ✓ | ✓ |
| [`clamp()`](./math/clamp.md) | `(int $value, int $min, int $max): mixed` | `mixed` | ✓ | ✓ |
| [`cos()`](./math/cos.md) | `(float $num): float` | `float` | ✓ | ✓ |
| [`cosh()`](./math/cosh.md) | `(float $num): float` | `float` | ✓ | ✓ |
| [`decbin()`](./math/decbin.md) | `(int $num): string` | `string` | ✓ | — |
| [`dechex()`](./math/dechex.md) | `(int $num): string` | `string` | ✓ | — |
| [`decoct()`](./math/decoct.md) | `(int $num): string` | `string` | ✓ | — |
| [`deg2rad()`](./math/deg2rad.md) | `(float $num): float` | `float` | ✓ | ✓ |
| [`exp()`](./math/exp.md) | `(float $num): float` | `float` | ✓ | ✓ |
| [`fdiv()`](./math/fdiv.md) | `(float $num1, float $num2): float` | `float` | ✓ | ✓ |
| [`floor()`](./math/floor.md) | `(float $num): float` | `float` | ✓ | ✓ |
| [`fmod()`](./math/fmod.md) | `(float $num1, float $num2): float` | `float` | ✓ | ✓ |
| [`getrandmax()`](./math/getrandmax.md) | `(): int` | `int` | ✓ | — |
| [`hexdec()`](./math/hexdec.md) | `(string $hex_string): mixed` | `mixed` | ✓ | — |
| [`hypot()`](./math/hypot.md) | `(float $x, float $y): float` | `float` | ✓ | ✓ |
| [`intdiv()`](./math/intdiv.md) | `(int $num1, int $num2): int` | `int` | ✓ | ✓ |
| [`is_finite()`](./math/is_finite.md) | `(float $num): bool` | `bool` | ✓ | ✓ |
| [`is_infinite()`](./math/is_infinite.md) | `(float $num): bool` | `bool` | ✓ | ✓ |
| [`is_nan()`](./math/is_nan.md) | `(float $num): bool` | `bool` | ✓ | ✓ |
| [`log()`](./math/log.md) | `(float $num, float $base = 2.718281828459045): float` | `float` | ✓ | ✓ |
| [`log10()`](./math/log10.md) | `(float $num): float` | `float` | ✓ | ✓ |
| [`log2()`](./math/log2.md) | `(float $num): float` | `float` | ✓ | ✓ |
| [`max()`](./math/max.md) | `(mixed $value, ...$values): mixed` | `mixed` | ✓ | ✓ |
| [`min()`](./math/min.md) | `(mixed $value, ...$values): mixed` | `mixed` | ✓ | ✓ |
| [`mt_rand()`](./math/mt_rand.md) | `(int $min, int $max): int` | `int` | ✓ | ✓ |
| [`octdec()`](./math/octdec.md) | `(string $octal_string): mixed` | `mixed` | ✓ | — |
| [`pi()`](./math/pi.md) | `(): float` | `float` | ✓ | ✓ |
| [`pow()`](./math/pow.md) | `(float $num, float $exponent): float` | `float` | ✓ | ✓ |
| [`rad2deg()`](./math/rad2deg.md) | `(float $num): float` | `float` | ✓ | ✓ |
| [`rand()`](./math/rand.md) | `(int $min, int $max): int` | `int` | ✓ | ✓ |
| [`random_int()`](./math/random_int.md) | `(int $min, int $max): int` | `int` | ✓ | ✓ |
| [`round()`](./math/round.md) | `(float $num, int $precision = 0, int $mode = 1): float` | `float` | ✓ | ✓ |
| [`sin()`](./math/sin.md) | `(float $num): float` | `float` | ✓ | ✓ |
| [`sinh()`](./math/sinh.md) | `(float $num): float` | `float` | ✓ | ✓ |
| [`sqrt()`](./math/sqrt.md) | `(float $num): float` | `float` | ✓ | ✓ |
| [`tan()`](./math/tan.md) | `(float $num): float` | `float` | ✓ | ✓ |
| [`tanh()`](./math/tanh.md) | `(float $num): float` | `float` | ✓ | ✓ |
