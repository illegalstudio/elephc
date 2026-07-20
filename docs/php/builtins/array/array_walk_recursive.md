---
title: "array_walk_recursive()"
description: "Applies a user function recursively to every member of an array."
sidebar:
  order: 46
---

## array_walk_recursive()

```php
function array_walk_recursive(array $array, callable $callback): void
```

Applies a user function recursively to every member of an array.

**Parameters**:
- `$array` (`array`), passed by reference
- `$callback` (`callable`)

**Returns**: `void`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `array_walk_recursive` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/array_walk_recursive.md).

