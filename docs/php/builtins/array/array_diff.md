---
title: "array_diff()"
description: "Computes the difference of arrays."
---

## array_diff()

```php
function array_diff(array $array, ...$arrays): array
```

Computes the difference of arrays.

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

For how `array_diff` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/array_diff.md).
