---
title: "ksort()"
description: "Sorts an array by key in ascending order, comparing keys under $flags."
---

## ksort()

```php
function ksort(array $array, int $flags = 0): bool
```

Sorts an array by key in ascending order, comparing keys under $flags.

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

For how `ksort` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/ksort.md).
