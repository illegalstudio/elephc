---
title: "gmp_testbit()"
description: "Tests a bit in a GMP number."
---

## gmp_testbit()

```php
function gmp_testbit(object|string|int $num, int $index): bool
```

Tests a bit in a GMP number.

**Parameters**:
- `$num` (`object|string|int`)
- `$index` (`int`)

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `gmp_testbit` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/gmp_testbit.md).
