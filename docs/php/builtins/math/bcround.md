---
title: "bcround()"
description: "Rounds an arbitrary-precision decimal number."
---

## bcround()

```php
function bcround(string $num, int $precision = 0, int $mode = 1): string
```

Rounds an arbitrary-precision decimal number.

**Parameters**:
- `$num` (`string`)
- `$precision` (`int`), default `0`, optional
- `$mode` (`int`), default `1`, optional

**Returns**: `string`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `bcround` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/bcround.md).
