---
title: "bcpow()"
description: "Raises an arbitrary-precision decimal number to an integral power."
---

## bcpow()

```php
function bcpow(string $num, string $exponent, ?int $scale = null): string
```

Raises an arbitrary-precision decimal number to an integral power.

**Parameters**:
- `$num` (`string`)
- `$exponent` (`string`)
- `$scale` (`?int`), default `null`, optional

**Returns**: `string`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `bcpow` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/bcpow.md).
