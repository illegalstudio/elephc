---
title: "array_filter()"
description: "Filters elements of an array using a callback function."
---

## array_filter()

```php
function array_filter(array $array, ?callable $callback = null, int $mode = 0): array
```

Filters elements of an array using a callback function.

**Parameters**:
- `$array` (`array`)
- `$callback` (`?callable`), default `null`, optional
- `$mode` (`int`), default `0`, optional

**Returns**: `array`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `array_filter` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/array_filter.md).
