---
title: "Random"
description: "PHP's random extension engines, Randomizer, interval boundaries, exceptions, and procedural API."
sidebar:
  order: 25
---

elephc implements PHP's `random` extension: the four `Random\Engine` implementations,
`Random\Randomizer`, the `Random\IntervalBoundary` enum, the extension's exception
hierarchy, and the procedural `mt_*` / `lcg_value` / `random_bytes` / `random_int`
functions. Engines are deterministic for a fixed seed and produce the same streams as
php-src 8.5, including the historical `MT_RAND_PHP` variant.

## Engines

```php
<?php
use Random\Engine\Mt19937;
use Random\Engine\PcgOneseq128XslRr64;
use Random\Engine\Xoshiro256StarStar;

$mt = new Mt19937(1234);
$pcg = new PcgOneseq128XslRr64("0123456789abcdef");
$xoshiro = new Xoshiro256StarStar(1234);

echo bin2hex($mt->generate()), "\n";
echo bin2hex($pcg->generate()), "\n";
echo bin2hex($xoshiro->generate()), "\n";
```

| Engine | State | Seed | Methods |
|---|---|---|---|
| `Random\Engine\Mt19937` | 624 × `uint32` | `?int`, `int $mode` | `generate`, `__serialize`, `__unserialize`, `__debugInfo` |
| `Random\Engine\PcgOneseq128XslRr64` | `uint128` | `string|int|null` (16-byte string) | `generate`, `jump`, serialization, debug info |
| `Random\Engine\Xoshiro256StarStar` | 4 × `uint64` | `string|int|null` (32-byte string) | `generate`, `jump`, `jumpLong`, serialization, debug info |
| `Random\Engine\Secure` | none | — | `generate` |

`Random\Engine` is a real interface, so a user class can implement it and be consumed by
`Random\Randomizer` exactly like a native engine. `Random\CryptoSafeEngine` extends it.

## Exceptions

`Random\RandomError` extends `Error` and `Random\BrokenRandomEngineError` extends it.
An engine that returns an empty string, or one that produces only biased results, makes
`Random\Randomizer` throw `Random\BrokenRandomEngineError` instead of looping forever.
`Random\RandomException` extends `Exception` and is raised when the operating-system
CSPRNG fails, so `random_bytes()` and `random_int()` throw it rather than returning
low-quality bytes.

## Randomizer

```php
<?php
use Random\Engine\Xoshiro256StarStar;
use Random\IntervalBoundary;
use Random\Randomizer;

$randomizer = new Randomizer(new Xoshiro256StarStar(2024));

var_dump($randomizer->getInt(1, 6));
var_dump($randomizer->getFloat(0.0, 1.0, IntervalBoundary::ClosedClosed));
var_dump(strlen($randomizer->getBytes(32)));
echo $randomizer->getBytesFromString("abcdef", 12), "\n";
echo implode(",", $randomizer->shuffleArray(range(1, 10))), "\n";
echo implode(",", $randomizer->pickArrayKeys(["a" => 1, "b" => 2, "c" => 3], 2)), "\n";
```

`Randomizer` stores its engine in a readonly property, defaults to a fresh
`Random\Engine\Secure`, and implements the same byte-accumulation, rejection-sampling and
gamma-section rules as php-src. A second `__construct()` call raises the readonly-property
`Error`; serialization round-trips the engine state.

## Procedural functions

```php
<?php
mt_srand(1234);
echo mt_rand(), "\n";
echo mt_rand(1, 6), "\n";
echo mt_getrandmax(), "\n";

mt_srand(1234, MT_RAND_MT19937);
echo mt_rand(0, 999), "\n";

echo strlen(random_bytes(32)), "\n";
var_dump(random_int(1, 6));
```

- `rand()` / `mt_rand()` take zero arguments or an inclusive `[min, max]` pair. `rand()`
  swaps inverted bounds; `mt_rand()` raises `ValueError`. Both consume the global
  seeded Mt19937 state, as do `array_rand()` and `shuffle()`.
- `array_rand()` accepts indexed and associative arrays, the single-key form
  (`array_rand($array)` / `array_rand($array, 1)`) and the multi-key form
  (`array_rand($array, $num)`), and draws from the same seeded global Mt19937 stream as
  `Randomizer::pickArrayKeys()`.
- `srand()` is an alias of `mt_srand()`. An unknown mode is clamped to
  `MT_RAND_MT19937`; `Mt19937::__construct()` rejects it instead.
- `random_int()` and `random_bytes()` use the operating-system CSPRNG, not the seeded
  Mt19937 state.
- `lcg_value()` is deprecated since PHP 8.4 in favour of
  `Random\Randomizer::getFloat()`.

## Serialization

Engine `__serialize()` output matches php-src byte for byte: Mt19937 emits 624
little-endian 8-character hex words plus `count` and `mode`; PCG emits its high and low
64-bit halves; Xoshiro emits its four words. `Random\Engine\Secure` is not serializable,
and `unserialize()` of malformed or all-zero Xoshiro state raises the same
`Invalid serialization data` exception.

## Known differences

- The `MT_RAND_PHP` and `lcg_value()` deprecations go through `trigger_error()`, so a
  custom error handler sees `E_USER_DEPRECATED` where php-src raises `E_DEPRECATED`. The
  message text, file, and line match php-src, including for the variant notices raised
  inside `mt_srand()` and `Mt19937::__construct()`, which report the user's call site.
- A `MT_RAND_PHP` read inside a constant expression (a `const`, a class constant, an enum
  case value, an attribute argument, or a default parameter) is reported as elephc's
  compile-time deprecation diagnostic, because those sites cannot contain a function call.
  Ordinary reads, including `switch` cases and `match` arms, emit the deprecation at run
  time, on stderr, exactly where the constant is fetched.
