---
title: "array_merge_recursive()"
description: "Recursively merges two arrays, combining scalar collisions into lists."
sidebar:
  order: 24
---

## array_merge_recursive()

```php
function array_merge_recursive(...$arrays): array
```

Recursively merges two arrays, combining scalar collisions into lists.

**Parameters**:
- `...$arrays` — variadic: collects excess arguments into `$arrays`.

**Returns**: `array`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `array_merge_recursive` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/array_merge_recursive.md).
