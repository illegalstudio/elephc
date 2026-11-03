---
title: "gmp_hamdist()"
description: "Hamming distance between two GMP numbers."
---

## gmp_hamdist()

```php
function gmp_hamdist(object|string|int $num1, object|string|int $num2): int
```

Hamming distance between two GMP numbers.

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

For how `gmp_hamdist` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/gmp_hamdist.md).
