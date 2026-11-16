---
title: "bcsub()"
description: "Subtracts two arbitrary-precision decimal numbers."
---

## bcsub()

```php
function bcsub(string $num1, string $num2, ?int $scale = null): string
```

Subtracts two arbitrary-precision decimal numbers.

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

For how `bcsub` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/bcsub.md).
