---
title: "array_unshift()"
description: "Prepends one or more elements to the beginning of an array."
---

## array_unshift()

```php
function array_unshift(array $array, ...$values): int
```

Prepends one or more elements to the beginning of an array.

**Parameters**:
- `$array` (`array`), passed by reference
- `...$values` - variadic: collects excess arguments into `$values`.

**Returns**: `int`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `array_unshift` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/array_unshift.md).
