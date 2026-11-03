---
title: "gmp_divexact()"
description: "Performs an exact GMP division."
---

## gmp_divexact()

```php
function gmp_divexact(object|string|int $num1, object|string|int $num2): object
```

Performs an exact GMP division.

**Parameters**:
- `$num1` (`object|string|int`)
- `$num2` (`object|string|int`)

**Returns**: `object`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `gmp_divexact` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/gmp_divexact.md).
