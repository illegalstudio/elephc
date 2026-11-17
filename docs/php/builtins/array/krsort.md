---
title: "krsort()"
description: "Sorts an array by key in descending order, comparing keys under $flags."
---

## krsort()

```php
function krsort(array $array, int $flags = 0): bool
```

Sorts an array by key in descending order, comparing keys under $flags.

**Parameters**:
- `$array` (`array`), passed by reference
- `$flags` (`int`), default `0`, optional

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `krsort` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/krsort.md).
