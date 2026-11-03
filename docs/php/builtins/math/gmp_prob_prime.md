---
title: "gmp_prob_prime()"
description: "Probable-prime test."
---

## gmp_prob_prime()

```php
function gmp_prob_prime(object|string|int $num, int $repetitions = 10): int
```

Probable-prime test.

**Parameters**:
- `$num` (`object|string|int`)
- `$repetitions` (`int`), default `10`, optional

**Returns**: `int`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `gmp_prob_prime` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/gmp_prob_prime.md).
