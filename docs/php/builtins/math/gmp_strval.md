---
title: "gmp_strval()"
description: "Formats a GMP number in a base."
---

## gmp_strval()

```php
function gmp_strval(object|string|int $num, int $base = 10): string
```

Formats a GMP number in a base.

**Parameters**:
- `$num` (`object|string|int`)
- `$base` (`int`), default `10`, optional

**Returns**: `string`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `gmp_strval` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/gmp_strval.md).
