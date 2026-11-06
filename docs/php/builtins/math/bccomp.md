---
title: "bccomp()"
description: "Compares two arbitrary-precision decimal numbers."
---

## bccomp()

```php
function bccomp(string $num1, string $num2, ?int $scale = null): int
```

Compares two arbitrary-precision decimal numbers.

**Parameters**:
- `$num1` (`string`)
- `$num2` (`string`)
- `$scale` (`?int`), default `null`, optional

**Returns**: `int`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `bccomp` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/bccomp.md).
