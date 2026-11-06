---
title: "gmp_scan1()"
description: "Finds the first set bit."
---

## gmp_scan1()

```php
function gmp_scan1(object|string|int $num1, int $start): int
```

Finds the first set bit.

**Parameters**:
- `$num1` (`object|string|int`)
- `$start` (`int`)

**Returns**: `int`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `gmp_scan1` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/gmp_scan1.md).
