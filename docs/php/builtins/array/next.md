---
title: "next()"
description: "Advances the array's internal pointer and returns the new element."
---

## next()

```php
function next(array &$array): mixed
```

Advances the array's internal pointer and returns the new element.

**Parameters**:
- `$array` (`array`), passed by reference

**Returns**: `mixed`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `next` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/next.md).
