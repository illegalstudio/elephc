---
title: "bcmod()"
description: "Returns the remainder of arbitrary-precision decimal division."
---

## bcmod()

```php
function bcmod(string $num1, string $num2, ?int $scale = null): string
```

Returns the remainder of arbitrary-precision decimal division.

**Parameters**:
- `$num1` (`string`)
- `$num2` (`string`)
- `$scale` (`?int`), default `null`, optional

**Returns**: `string`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `bcmod` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/bcmod.md).
