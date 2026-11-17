---
title: "array_merge()"
description: "Merges the elements of two arrays."
---

## array_merge()

```php
function array_merge(...$arrays): array
```

Merges the elements of two arrays.

**Parameters**:
- `...$arrays` - variadic: collects excess arguments into `$arrays`.

**Returns**: `array`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `array_merge` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/array_merge.md).
