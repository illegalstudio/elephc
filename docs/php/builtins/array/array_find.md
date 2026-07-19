---
title: "array_find()"
description: "Returns the first element satisfying a predicate callback, or null."
sidebar:
  order: 12
---

## array_find()

```php
function array_find(mixed $array, mixed $callback): mixed
```

Returns the first element satisfying a predicate callback, or null.

**Parameters**:
- `$array` (`mixed`)
- `$callback` (`mixed`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `array_find` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/array_find.md).

