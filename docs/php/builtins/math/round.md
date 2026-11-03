---
title: "round()"
description: "Rounds a float."
---

## round()

```php
function round(float $num, int $precision = 0, int $mode = 1): float
```

Rounds a float.

**Parameters**:
- `$num` (`float`)
- `$precision` (`int`), default `0`, optional
- `$mode` (`int`), default `1`, optional

**Returns**: `float`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `round` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/round.md).
