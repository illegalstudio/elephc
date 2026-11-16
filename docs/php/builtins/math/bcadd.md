---
title: "bcadd()"
description: "Adds two arbitrary-precision decimal numbers."
---

## bcadd()

```php
function bcadd(string $num1, string $num2, ?int $scale = null): string
```

Adds two arbitrary-precision decimal numbers.

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

For how `bcadd` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/bcadd.md).
