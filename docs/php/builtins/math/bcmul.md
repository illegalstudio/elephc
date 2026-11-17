---
title: "bcmul()"
description: "Multiplies two arbitrary-precision decimal numbers."
---

## bcmul()

```php
function bcmul(string $num1, string $num2, ?int $scale = null): string
```

Multiplies two arbitrary-precision decimal numbers.

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

For how `bcmul` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/bcmul.md).
