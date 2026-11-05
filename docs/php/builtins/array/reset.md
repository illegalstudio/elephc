---
title: "reset()"
description: "Rewinds the array's internal pointer to the first element and returns it."
---

## reset()

```php
function reset(array &$array): mixed
```

Rewinds the array's internal pointer to the first element and returns it.

**Parameters**:
- `$array` (`array`), passed by reference

**Returns**: `mixed`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `reset` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/reset.md).
