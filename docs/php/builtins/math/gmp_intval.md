---
title: "gmp_intval()"
description: "Converts a GMP number to an integer."
---

## gmp_intval()

```php
function gmp_intval(object|string|int $num): int
```

Converts a GMP number to an integer.

**Parameters**:
- `$num` (`object|string|int`)

**Returns**: `int`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `gmp_intval` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/gmp_intval.md).
