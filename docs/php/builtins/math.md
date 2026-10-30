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
| [`bcadd()`](./math/bcadd.md) | `(string $num1, string $num2, ?int $scale = null): string` | `string` | ✓ | ✓ |
| [`bcceil()`](./math/bcceil.md) | `(string $num): string` | `string` | ✓ | ✓ |
| [`bccomp()`](./math/bccomp.md) | `(string $num1, string $num2, ?int $scale = null): int` | `int` | ✓ | ✓ |
| [`bcdiv()`](./math/bcdiv.md) | `(string $num1, string $num2, ?int $scale = null): string` | `string` | ✓ | ✓ |
| [`bcdivmod()`](./math/bcdivmod.md) | `(string $num1, string $num2, ?int $scale = null): array` | `array` | ✓ | ✓ |
| [`bcfloor()`](./math/bcfloor.md) | `(string $num): string` | `string` | ✓ | ✓ |
| [`bcmod()`](./math/bcmod.md) | `(string $num1, string $num2, ?int $scale = null): string` | `string` | ✓ | ✓ |
| [`bcmul()`](./math/bcmul.md) | `(string $num1, string $num2, ?int $scale = null): string` | `string` | ✓ | ✓ |
| [`bcpow()`](./math/bcpow.md) | `(string $num, string $exponent, ?int $scale = null): string` | `string` | ✓ | ✓ |
| [`bcpowmod()`](./math/bcpowmod.md) | `(string $num, string $exponent, string $modulus, ?int $scale = null): string` | `string` | ✓ | ✓ |
| [`bcround()`](./math/bcround.md) | `(string $num, int $precision = 0, int $mode = 1): string` | `string` | ✓ | ✓ |
| [`bcscale()`](./math/bcscale.md) | `(?int $scale = null): int` | `int` | ✓ | ✓ |
| [`bcsqrt()`](./math/bcsqrt.md) | `(string $num, ?int $scale = null): string` | `string` | ✓ | ✓ |
| [`bcsub()`](./math/bcsub.md) | `(string $num1, string $num2, ?int $scale = null): string` | `string` | ✓ | ✓ |
| [`bindec()`](./math/bindec.md) | `(string $binary_string): mixed` | `mixed` | ✓ | - |
| [`ceil()`](./math/ceil.md) | `(float $num): float` | `float` | ✓ | ✓ |
| [`clamp()`](./math/clamp.md) | `(int $value, int $min, int $max): mixed` | `mixed` | ✓ | ✓ |
| [`cos()`](./math/cos.md) | `(float $num): float` | `float` | ✓ | ✓ |
| [`cosh()`](./math/cosh.md) | `(float $num): float` | `float` | ✓ | ✓ |
| [`decbin()`](./math/decbin.md) | `(int $num): string` | `string` | ✓ | - |
| [`dechex()`](./math/dechex.md) | `(int $num): string` | `string` | ✓ | - |
| [`decoct()`](./math/decoct.md) | `(int $num): string` | `string` | ✓ | - |
| [`deg2rad()`](./math/deg2rad.md) | `(float $num): float` | `float` | ✓ | ✓ |
| [`exp()`](./math/exp.md) | `(float $num): float` | `float` | ✓ | ✓ |
| [`fdiv()`](./math/fdiv.md) | `(float $num1, float $num2): float` | `float` | ✓ | ✓ |
| [`floor()`](./math/floor.md) | `(float $num): float` | `float` | ✓ | ✓ |
| [`fmod()`](./math/fmod.md) | `(float $num1, float $num2): float` | `float` | ✓ | ✓ |
| [`getrandmax()`](./math/getrandmax.md) | `(): int` | `int` | ✓ | - |
| [`gmp_abs()`](./math/gmp_abs.md) | `(object|string|int $num): object` | `object` | ✓ | ✓ |
| [`gmp_add()`](./math/gmp_add.md) | `(object|string|int $num1, object|string|int $num2): object` | `object` | ✓ | ✓ |
| [`gmp_and()`](./math/gmp_and.md) | `(object|string|int $num1, object|string|int $num2): object` | `object` | ✓ | ✓ |
| [`gmp_binomial()`](./math/gmp_binomial.md) | `(object|string|int $num, int $k): object` | `object` | ✓ | ✓ |
| [`gmp_clrbit()`](./math/gmp_clrbit.md) | `(object $num, int $index): void` | `void` | ✓ | ✓ |
| [`gmp_cmp()`](./math/gmp_cmp.md) | `(object|string|int $num1, object|string|int $num2): int` | `int` | ✓ | ✓ |
| [`gmp_com()`](./math/gmp_com.md) | `(object|string|int $num): object` | `object` | ✓ | ✓ |
| [`gmp_div()`](./math/gmp_div.md) | `(object|string|int $num1, object|string|int $num2, int $rounding_mode = 0): object` | `object` | ✓ | ✓ |
| [`gmp_div_q()`](./math/gmp_div_q.md) | `(object|string|int $num1, object|string|int $num2, int $rounding_mode = 0): object` | `object` | ✓ | ✓ |
| [`gmp_div_qr()`](./math/gmp_div_qr.md) | `(object|string|int $num1, object|string|int $num2, int $rounding_mode = 0): array` | `array` | ✓ | ✓ |
| [`gmp_div_r()`](./math/gmp_div_r.md) | `(object|string|int $num1, object|string|int $num2, int $rounding_mode = 0): object` | `object` | ✓ | ✓ |
| [`gmp_divexact()`](./math/gmp_divexact.md) | `(object|string|int $num1, object|string|int $num2): object` | `object` | ✓ | ✓ |
| [`gmp_export()`](./math/gmp_export.md) | `(object|string|int $num, int $word_size = 1, int $flags = 17): string` | `string` | ✓ | ✓ |
| [`gmp_fact()`](./math/gmp_fact.md) | `(object|string|int $num): object` | `object` | ✓ | ✓ |
| [`gmp_gcd()`](./math/gmp_gcd.md) | `(object|string|int $num1, object|string|int $num2): object` | `object` | ✓ | ✓ |
| [`gmp_gcdext()`](./math/gmp_gcdext.md) | `(object|string|int $num1, object|string|int $num2): array` | `array` | ✓ | ✓ |
| [`gmp_hamdist()`](./math/gmp_hamdist.md) | `(object|string|int $num1, object|string|int $num2): int` | `int` | ✓ | ✓ |
| [`gmp_import()`](./math/gmp_import.md) | `(string $data, int $word_size = 1, int $flags = 17): object` | `object` | ✓ | ✓ |
| [`gmp_init()`](./math/gmp_init.md) | `(string|int $num, int $base = 0): object` | `object` | ✓ | ✓ |
| [`gmp_intval()`](./math/gmp_intval.md) | `(object|string|int $num): int` | `int` | ✓ | ✓ |
| [`gmp_invert()`](./math/gmp_invert.md) | `(object|string|int $num1, object|string|int $num2): object|false` | `object|false` | ✓ | ✓ |
| [`gmp_jacobi()`](./math/gmp_jacobi.md) | `(object|string|int $num1, object|string|int $num2): int` | `int` | ✓ | ✓ |
| [`gmp_kronecker()`](./math/gmp_kronecker.md) | `(object|string|int $num1, object|string|int $num2): int` | `int` | ✓ | ✓ |
| [`gmp_lcm()`](./math/gmp_lcm.md) | `(object|string|int $num1, object|string|int $num2): object` | `object` | ✓ | ✓ |
| [`gmp_legendre()`](./math/gmp_legendre.md) | `(object|string|int $num1, object|string|int $num2): int` | `int` | ✓ | ✓ |
| [`gmp_mod()`](./math/gmp_mod.md) | `(object|string|int $num1, object|string|int $num2): object` | `object` | ✓ | ✓ |
| [`gmp_mul()`](./math/gmp_mul.md) | `(object|string|int $num1, object|string|int $num2): object` | `object` | ✓ | ✓ |
| [`gmp_neg()`](./math/gmp_neg.md) | `(object|string|int $num): object` | `object` | ✓ | ✓ |
| [`gmp_nextprime()`](./math/gmp_nextprime.md) | `(object|string|int $num): object` | `object` | ✓ | ✓ |
| [`gmp_or()`](./math/gmp_or.md) | `(object|string|int $num1, object|string|int $num2): object` | `object` | ✓ | ✓ |
| [`gmp_perfect_power()`](./math/gmp_perfect_power.md) | `(object|string|int $num): bool` | `bool` | ✓ | ✓ |
| [`gmp_perfect_square()`](./math/gmp_perfect_square.md) | `(object|string|int $num): bool` | `bool` | ✓ | ✓ |
| [`gmp_popcount()`](./math/gmp_popcount.md) | `(object|string|int $num): int` | `int` | ✓ | ✓ |
| [`gmp_pow()`](./math/gmp_pow.md) | `(object|string|int $num, int $exponent): object` | `object` | ✓ | ✓ |
| [`gmp_powm()`](./math/gmp_powm.md) | `(object|string|int $num, object|string|int $exponent, object|string|int $modulus): object` | `object` | ✓ | ✓ |
| [`gmp_prob_prime()`](./math/gmp_prob_prime.md) | `(object|string|int $num, int $repetitions = 10): int` | `int` | ✓ | ✓ |
| [`gmp_random_bits()`](./math/gmp_random_bits.md) | `(int $bits): object` | `object` | ✓ | ✓ |
| [`gmp_random_range()`](./math/gmp_random_range.md) | `(object|string|int $min, object|string|int $max): object` | `object` | ✓ | ✓ |
| [`gmp_random_seed()`](./math/gmp_random_seed.md) | `(object|string|int $seed): void` | `void` | ✓ | ✓ |
| [`gmp_root()`](./math/gmp_root.md) | `(object|string|int $num, int $nth): object` | `object` | ✓ | ✓ |
| [`gmp_rootrem()`](./math/gmp_rootrem.md) | `(object|string|int $num, int $nth): array` | `array` | ✓ | ✓ |
| [`gmp_scan0()`](./math/gmp_scan0.md) | `(object|string|int $num1, int $start): int` | `int` | ✓ | ✓ |
| [`gmp_scan1()`](./math/gmp_scan1.md) | `(object|string|int $num1, int $start): int` | `int` | ✓ | ✓ |
| [`gmp_setbit()`](./math/gmp_setbit.md) | `(object $num, int $index, bool $value = true): void` | `void` | ✓ | ✓ |
| [`gmp_sign()`](./math/gmp_sign.md) | `(object|string|int $num): int` | `int` | ✓ | ✓ |
| [`gmp_sqrt()`](./math/gmp_sqrt.md) | `(object|string|int $num): object` | `object` | ✓ | ✓ |
| [`gmp_sqrtrem()`](./math/gmp_sqrtrem.md) | `(object|string|int $num): array` | `array` | ✓ | ✓ |
| [`gmp_strval()`](./math/gmp_strval.md) | `(object|string|int $num, int $base = 10): string` | `string` | ✓ | ✓ |
| [`gmp_sub()`](./math/gmp_sub.md) | `(object|string|int $num1, object|string|int $num2): object` | `object` | ✓ | ✓ |
| [`gmp_testbit()`](./math/gmp_testbit.md) | `(object|string|int $num, int $index): bool` | `bool` | ✓ | ✓ |
| [`gmp_xor()`](./math/gmp_xor.md) | `(object|string|int $num1, object|string|int $num2): object` | `object` | ✓ | ✓ |
| [`hexdec()`](./math/hexdec.md) | `(string $hex_string): mixed` | `mixed` | ✓ | - |
| [`hypot()`](./math/hypot.md) | `(float $x, float $y): float` | `float` | ✓ | ✓ |
| [`intdiv()`](./math/intdiv.md) | `(int $num1, int $num2): int` | `int` | ✓ | ✓ |
| [`is_finite()`](./math/is_finite.md) | `(float $num): bool` | `bool` | ✓ | ✓ |
| [`is_infinite()`](./math/is_infinite.md) | `(float $num): bool` | `bool` | ✓ | ✓ |
| [`is_nan()`](./math/is_nan.md) | `(float $num): bool` | `bool` | ✓ | ✓ |
| [`lcg_value()`](./math/lcg_value.md) | `(): float` | `float` | ✓ | - |
| [`log()`](./math/log.md) | `(float $num, float $base = 2.718281828459045): float` | `float` | ✓ | ✓ |
| [`log10()`](./math/log10.md) | `(float $num): float` | `float` | ✓ | ✓ |
| [`log2()`](./math/log2.md) | `(float $num): float` | `float` | ✓ | ✓ |
| [`max()`](./math/max.md) | `(mixed $value, ...$values): mixed` | `mixed` | ✓ | ✓ |
| [`min()`](./math/min.md) | `(mixed $value, ...$values): mixed` | `mixed` | ✓ | ✓ |
| [`mt_getrandmax()`](./math/mt_getrandmax.md) | `(): int` | `int` | ✓ | - |
| [`mt_rand()`](./math/mt_rand.md) | `(int $min, int $max): int` | `int` | ✓ | ✓ |
| [`mt_srand()`](./math/mt_srand.md) | `(?int $seed = null, int $mode = 0): void` | `void` | ✓ | - |
| [`octdec()`](./math/octdec.md) | `(string $octal_string): mixed` | `mixed` | ✓ | - |
| [`pi()`](./math/pi.md) | `(): float` | `float` | ✓ | ✓ |
| [`pow()`](./math/pow.md) | `(float $num, float $exponent): float` | `float` | ✓ | ✓ |
| [`rad2deg()`](./math/rad2deg.md) | `(float $num): float` | `float` | ✓ | ✓ |
| [`rand()`](./math/rand.md) | `(int $min, int $max): int` | `int` | ✓ | ✓ |
| [`random_bytes()`](./math/random_bytes.md) | `(int $length): string` | `string` | ✓ | - |
| [`random_int()`](./math/random_int.md) | `(int $min, int $max): int` | `int` | ✓ | ✓ |
| [`round()`](./math/round.md) | `(float $num, int $precision = 0, int $mode = 1): float` | `float` | ✓ | ✓ |
| [`sin()`](./math/sin.md) | `(float $num): float` | `float` | ✓ | ✓ |
| [`sinh()`](./math/sinh.md) | `(float $num): float` | `float` | ✓ | ✓ |
| [`sqrt()`](./math/sqrt.md) | `(float $num): float` | `float` | ✓ | ✓ |
| [`srand()`](./math/srand.md) | `(?int $seed = null, int $mode = 0): void` | `void` | ✓ | - |
| [`tan()`](./math/tan.md) | `(float $num): float` | `float` | ✓ | ✓ |
| [`tanh()`](./math/tanh.md) | `(float $num): float` | `float` | ✓ | ✓ |
