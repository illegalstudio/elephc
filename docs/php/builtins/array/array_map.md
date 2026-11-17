---
title: "array_map()"
description: "Applies a callback to the elements of an array."
---

## array_map()

```php
function array_map(?callable $callback, array $array, ...$arrays): array
```

Applies a callback to the elements of an array.

**Parameters**:
- `$callback` (`?callable`)
- `$array` (`array`)
- `...$arrays` - variadic: collects excess arguments into `$arrays`.

**Returns**: `array`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `array_map` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/array_map.md).
