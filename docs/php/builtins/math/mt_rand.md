---
title: "mt_rand()"
description: "Generate a random value via the Mersenne Twister Random Number Generator."
---

## mt_rand()

```php
function mt_rand(int $min, int $max): int
```

Generate a random value via the Mersenne Twister Random Number Generator.

**Parameters**:
- `$min` (`int`)
- `$max` (`int`)

**Returns**: `int`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mt_rand` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/mt_rand.md).
