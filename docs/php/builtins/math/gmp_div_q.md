---
title: "gmp_div_q()"
description: "Divides two GMP numbers."
---

## gmp_div_q()

```php
function gmp_div_q(object|string|int $num1, object|string|int $num2, int $rounding_mode = 0): object
```

Divides two GMP numbers.

**Parameters**:
- `$num1` (`object|string|int`)
- `$num2` (`object|string|int`)
- `$rounding_mode` (`int`), default `0`, optional

**Returns**: `object`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `gmp_div_q` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/gmp_div_q.md).
