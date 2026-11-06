---
title: "gmp_add()"
description: "Adds two GMP numbers."
---

## gmp_add()

```php
function gmp_add(object|string|int $num1, object|string|int $num2): object
```

Adds two GMP numbers.

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

For how `gmp_add` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/gmp_add.md).
