---
title: "gmp_nextprime()"
description: "Returns the next prime greater than a GMP number."
---

## gmp_nextprime()

```php
function gmp_nextprime(object|string|int $num): object
```

Returns the next prime greater than a GMP number.

**Parameters**:
- `$num` (`object|string|int`)

**Returns**: `object`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `gmp_nextprime` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/gmp_nextprime.md).
