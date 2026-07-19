---
title: "array_uintersect()"
description: "Computes the intersection of arrays using a callback comparator."
sidebar:
  order: 41
---

## array_uintersect()

```php
function array_uintersect(array $array1, array $array2, callable $callback): array
```

Computes the intersection of arrays using a callback comparator.

**Parameters**:
- `$array1` (`array`)
- `$array2` (`array`)
- `$callback` (`callable`)

**Returns**: `array`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `array_uintersect` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/array_uintersect.md).

