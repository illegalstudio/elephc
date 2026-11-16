---
title: "random_int()"
description: "Get a cryptographically secure, uniformly selected integer."
---

## random_int()

```php
function random_int(int $min, int $max): int
```

Get a cryptographically secure, uniformly selected integer.

**Parameters**:
- `$min` (`int`)
- `$max` (`int`)

**Returns**: `int`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `random_int` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/random_int.md).
