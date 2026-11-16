---
title: "gmp_setbit()"
description: "Sets or clears a bit in a GMP number."
---

## gmp_setbit()

```php
function gmp_setbit(object $num, int $index, bool $value = true): void
```

Sets or clears a bit in a GMP number.

**Parameters**:
- `$num` (`object`)
- `$index` (`int`)
- `$value` (`bool`), default `true`, optional

**Returns**: `void`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `gmp_setbit` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/gmp_setbit.md).
