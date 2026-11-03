---
title: "gmp_cmp()"
description: "Compares two GMP numbers."
---

## gmp_cmp()

```php
function gmp_cmp(object|string|int $num1, object|string|int $num2): int
```

Compares two GMP numbers.

**Parameters**:
- `$num1` (`object|string|int`)
- `$num2` (`object|string|int`)

**Returns**: `int`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `gmp_cmp` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/gmp_cmp.md).
