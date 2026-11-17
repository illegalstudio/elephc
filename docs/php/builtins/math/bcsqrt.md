---
title: "bcsqrt()"
description: "Returns the square root of an arbitrary-precision decimal number."
---

## bcsqrt()

```php
function bcsqrt(string $num, ?int $scale = null): string
```

Returns the square root of an arbitrary-precision decimal number.

**Parameters**:
- `$num` (`string`)
- `$scale` (`?int`), default `null`, optional

**Returns**: `string`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `bcsqrt` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/bcsqrt.md).
