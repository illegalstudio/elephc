---
title: "uasort()"
description: "Sorts an array with a user-defined comparison function and maintains index association."
---

## uasort()

```php
function uasort(array &$array, callable $callback): bool
```

Sorts an array with a user-defined comparison function and maintains index association.

**Parameters**:
- `$array` (`array`), passed by reference
- `$callback` (`callable`)

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `uasort` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/uasort.md).
