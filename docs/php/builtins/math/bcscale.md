---
title: "bcscale()"
description: "Gets or sets the process-wide default BCMath scale."
---

## bcscale()

```php
function bcscale(?int $scale = null): int
```

Gets or sets the process-wide default BCMath scale.

**Parameters**:
- `$scale` (`?int`), default `null`, optional

**Returns**: `int`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `bcscale` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/bcscale.md).
