---
title: "array_is_list()"
description: "Checks whether an array is a list (sequential 0-based integer keys)."
sidebar:
  order: 17
---

## array_is_list()

```php
function array_is_list(mixed $array): bool
```

Checks whether an array is a list (sequential 0-based integer keys).

**Parameters**:
- `$array` (`mixed`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `array_is_list` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/array_is_list.md).
