---
title: "clamp()"
description: "Clamps a value to be within a specified range."
---

## clamp()

```php
function clamp(int $value, int $min, int $max): mixed
```

Clamps a value to be within a specified range.

**Parameters**:
- `$value` (`int`)
- `$min` (`int`)
- `$max` (`int`)

**Returns**: `mixed`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `clamp` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/clamp.md).
