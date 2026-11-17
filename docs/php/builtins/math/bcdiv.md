---
title: "bcdiv()"
description: "Divides two arbitrary-precision decimal numbers."
---

## bcdiv()

```php
function bcdiv(string $num1, string $num2, ?int $scale = null): string
```

Divides two arbitrary-precision decimal numbers.

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

For how `bcdiv` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/bcdiv.md).
