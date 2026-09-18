---
title: "array_multisort()"
description: "Sorts exactly two equal-length indexed arrays in ascending tuple order. AOT accepts either two concrete integer arrays or two boxed scalar arrays, not a mixed pair. Concrete string/float arrays, sort flags, associative arrays, and eval are unsupported."
sidebar:
  order: 26
---

## array_multisort()

```php
function array_multisort(array $array1, array $array2): bool
```

Sorts exactly two equal-length indexed arrays in ascending tuple order. AOT accepts either two concrete integer arrays or two boxed scalar arrays, not a mixed pair. Concrete string/float arrays, sort flags, associative arrays, and eval are unsupported.

**Parameters**:
- `$array1` (`array`), passed by reference
- `$array2` (`array`), passed by reference

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `array_multisort` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/array_multisort.md).
