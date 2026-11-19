---
title: "gmp_prevprime()"
description: "Returns the largest prime smaller than a GMP number."
---

## gmp_prevprime()

```php
function gmp_prevprime(object|string|int $num, bool $definitely_prime = null): object
```

Returns the largest prime smaller than a GMP number.

**Parameters**:
- `$num` (`object|string|int`)
- `$definitely_prime` (`bool`), passed by reference, default `null`, optional

**Returns**: `object`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `gmp_prevprime` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/gmp_prevprime.md).
