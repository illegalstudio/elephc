---
title: "array_udiff()"
description: "Computes the difference of arrays using a callback comparator."
sidebar:
  order: 40
---

## array_udiff()

```php
function array_udiff(array $array1, array $array2, callable $callback): array
```

Computes the difference of arrays using a callback comparator.

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

For how `array_udiff` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/array_udiff.md).
