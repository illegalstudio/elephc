---
title: "gmp_random_range()"
description: "Generates a random GMP number in a range."
---

## gmp_random_range()

```php
function gmp_random_range(object|string|int $min, object|string|int $max): object
```

Generates a random GMP number in a range.

**Parameters**:
- `$min` (`object|string|int`)
- `$max` (`object|string|int`)

**Returns**: `object`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `gmp_random_range` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/gmp_random_range.md).
