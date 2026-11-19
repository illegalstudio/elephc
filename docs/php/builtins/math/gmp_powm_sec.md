---
title: "gmp_powm_sec()"
description: "Side-channel quiet modular exponentiation."
---

## gmp_powm_sec()

```php
function gmp_powm_sec(object|string|int $num, object|string|int $exponent, object|string|int $modulus): object
```

Side-channel quiet modular exponentiation.

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

For how `gmp_powm_sec` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/gmp_powm_sec.md).
