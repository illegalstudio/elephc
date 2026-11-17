---
title: "imagesetinterpolation()"
description: "Selects the interpolation method used when resampling."
---

## imagesetinterpolation()

```php
function imagesetinterpolation(mixed $image, int $method = IMG_BILINEAR_FIXED): bool
```

Selects the interpolation method used when resampling.

**Parameters**:
- `$image` (`mixed`)
- `$method` (`int`), default `IMG_BILINEAR_FIXED`, optional

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imagesetinterpolation` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imagesetinterpolation.md).
