---
title: "gmp_random_seed()"
description: "Seeds PHP's GMP random generator."
---

## gmp_random_seed()

```php
function gmp_random_seed(object|string|int $seed): void
```

Seeds PHP's GMP random generator.

**Parameters**:
- `$seed` (`object|string|int`)

**Returns**: `void`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `gmp_random_seed` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/gmp_random_seed.md).
