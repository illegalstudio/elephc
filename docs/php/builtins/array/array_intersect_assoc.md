---
title: "array_intersect_assoc()"
description: "Computes the intersection of arrays with additional index check."
sidebar:
  order: 15
---

## array_intersect_assoc()

```php
function array_intersect_assoc(array $array, ...$arrays): mixed
```

Computes the intersection of arrays with additional index check.

**Parameters**:
- `$array` (`array`)
- `...$arrays` — variadic: collects excess arguments into `$arrays`.

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `array_intersect_assoc` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/array_intersect_assoc.md).

