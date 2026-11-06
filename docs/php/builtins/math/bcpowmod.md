---
title: "bcpowmod()"
description: "Returns an arbitrary-precision integral modular power."
---

## bcpowmod()

```php
function bcpowmod(string $num, string $exponent, string $modulus, ?int $scale = null): string
```

Returns an arbitrary-precision integral modular power.

**Parameters**:
- `$num` (`string`)
- `$exponent` (`string`)
- `$modulus` (`string`)
- `$scale` (`?int`), default `null`, optional

**Returns**: `string`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `bcpowmod` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/bcpowmod.md).
