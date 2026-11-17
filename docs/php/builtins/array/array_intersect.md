---
title: "array_intersect()"
description: "Computes the intersection of arrays."
---

## array_intersect()

```php
function array_intersect(array $array, ...$arrays): array
```

Computes the intersection of arrays.

**Parameters**:
- `$array` (`array`)
- `...$arrays` - variadic: collects excess arguments into `$arrays`.

**Returns**: `array`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `array_intersect` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/array_intersect.md).
