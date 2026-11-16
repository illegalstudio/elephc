---
title: "mt_srand()"
description: "Seed the Mersenne Twister random number generator."
---

## mt_srand()

```php
function mt_srand(?int $seed = null, int $mode = 0): void
```

Seed the Mersenne Twister random number generator.

**Parameters**:
- `$seed` (`?int`), default `null`, optional
- `$mode` (`int`), default `0`, optional

**Returns**: `void`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mt_srand` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/mt_srand.md).
