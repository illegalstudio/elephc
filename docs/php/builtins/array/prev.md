---
title: "prev()"
description: "Rewinds the array's internal pointer and returns the new element."
---

## prev()

```php
function prev(array $array): mixed
```

Rewinds the array's internal pointer and returns the new element.

**Parameters**:
- `$array` (`array`), passed by reference

**Returns**: `mixed`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `prev` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/prev.md).
