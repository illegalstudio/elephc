---
title: "array_uintersect()"
description: "Returns entries from the first of exactly two arrays whose values occur in the second according to an integer-cast callback comparator, preserving keys."
---

## array_uintersect()

```php
function array_uintersect(array $array1, array $array2, callable $callback): array
```

Returns entries from the first of exactly two arrays whose values occur in the second according to an integer-cast callback comparator, preserving keys.

**Parameters**:
- `$array1` (`array`)
- `$array2` (`array`)
- `$callback` (`callable`)

**Returns**: `array`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `array_uintersect` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/array_uintersect.md).
