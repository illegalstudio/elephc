---
title: "bcdivmod()"
description: "Returns the quotient and remainder of arbitrary-precision division."
---

## bcdivmod()

```php
function bcdivmod(string $num1, string $num2, ?int $scale = null): array
```

Returns the quotient and remainder of arbitrary-precision division.

**Parameters**:
- `$num1` (`string`)
- `$num2` (`string`)
- `$scale` (`?int`), default `null`, optional

**Returns**: `array`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `bcdivmod` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/bcdivmod.md).
