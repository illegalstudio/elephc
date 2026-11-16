---
title: "gmp_pow()"
description: "Raises a GMP number to an integer power."
---

## gmp_pow()

```php
function gmp_pow(object|string|int $num, int $exponent): object
```

Raises a GMP number to an integer power.

**Parameters**:
- `$num` (`object|string|int`)
- `$exponent` (`int`)

**Returns**: `object`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `gmp_pow` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/gmp_pow.md).
