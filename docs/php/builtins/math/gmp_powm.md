---
title: "gmp_powm()"
description: "Modular exponentiation."
---

## gmp_powm()

```php
function gmp_powm(object|string|int $num, object|string|int $exponent, object|string|int $modulus): object
```

Modular exponentiation.

**Parameters**:
- `$num` (`object|string|int`)
- `$exponent` (`object|string|int`)
- `$modulus` (`object|string|int`)

**Returns**: `object`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `gmp_powm` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/gmp_powm.md).
