---
title: "gmp_invert()"
description: "Modular inverse."
---

## gmp_invert()

```php
function gmp_invert(object|string|int $num1, object|string|int $num2): object|false
```

Modular inverse.

**Parameters**:
- `$num1` (`object|string|int`)
- `$num2` (`object|string|int`)

**Returns**: `object|false`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `gmp_invert` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/gmp_invert.md).
