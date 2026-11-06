---
title: "array_push()"
description: "Pushes one or more elements onto the end of array."
---

## array_push()

```php
function array_push(array &$array, ...$values): int
```

Pushes one or more elements onto the end of array.

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

For how `array_push` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/array_push.md).
