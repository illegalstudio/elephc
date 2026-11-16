---
title: "array_rand()"
description: "Pick one or more random keys out of an array."
---

## array_rand()

```php
function array_rand(array $array, int $num = 1): int|string|array
```

Pick one or more random keys out of an array.

**Parameters**:
- `$array` (`array`)
- `$num` (`int`), default `1`, optional

**Returns**: `int|string|array`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `array_rand` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/array_rand.md).
