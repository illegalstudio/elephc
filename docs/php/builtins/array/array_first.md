---
title: "array_first()"
description: "Returns the first value of an array in insertion order, or null when it is empty."
sidebar:
  order: 14
---

## array_first()

```php
function array_first(array $array): mixed
```

Returns the first value of an array in insertion order, or null when it is empty.

**Parameters**:
- `$array` (`array`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `array_first` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/array_first.md).
