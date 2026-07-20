---
title: "array_multisort()"
description: "Sorts multiple arrays or multi-dimensional arrays."
sidebar:
  order: 25
---

## array_multisort()

```php
function array_multisort(array $array1, int $array2): bool
```

Sorts multiple arrays or multi-dimensional arrays.

**Parameters**:
- `$array1` (`array`), passed by reference
- `$array2` (`int`), passed by reference

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `array_multisort` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/array_multisort.md).

