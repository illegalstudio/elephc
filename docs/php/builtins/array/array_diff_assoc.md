---
title: "array_diff_assoc()"
description: "Computes the difference of arrays with additional index check."
sidebar:
  order: 7
---

## array_diff_assoc()

```php
function array_diff_assoc(array $array, ...$arrays): mixed
```

Computes the difference of arrays with additional index check.

**Parameters**:
- `$array` (`array`)
- `...$arrays` — variadic: collects excess arguments into `$arrays`.

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `array_diff_assoc` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/array_diff_assoc.md).

