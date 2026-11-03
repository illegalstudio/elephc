---
title: "usort()"
description: "Sorts an array by values using a user-defined comparison function."
---

## usort()

```php
function usort(array $array, callable $callback): bool
```

Sorts an array by values using a user-defined comparison function.

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

For how `usort` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/usort.md).
