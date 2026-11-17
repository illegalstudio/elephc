---
title: "gmp_div()"
description: "Alias of gmp_div_q()."
---

## gmp_div()

```php
function gmp_div(object|string|int $num1, object|string|int $num2, int $rounding_mode = 0): object
```

Alias of gmp_div_q().

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

For how `gmp_div` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/gmp_div.md).
