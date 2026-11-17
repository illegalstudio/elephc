---
title: "array_diff_key()"
description: "Computes the difference of arrays using keys for comparison."
---

## array_diff_key()

```php
function array_diff_key(array $array, ...$arrays): array
```

Computes the difference of arrays using keys for comparison.

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

For how `array_diff_key` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/array_diff_key.md).
