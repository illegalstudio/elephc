---
title: "end()"
description: "Moves the array's internal pointer to the last element and returns it."
---

## end()

```php
function end(array &$array): mixed
```

Moves the array's internal pointer to the last element and returns it.

**Parameters**:
- `$array` (`array`), passed by reference

**Returns**: `mixed`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `end` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/end.md).
