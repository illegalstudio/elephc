---
title: "imagescale()"
description: "Returns a copy of an image scaled to a new size."
---

## imagescale()

```php
function imagescale(mixed $image, int $width, int $height = -1, int $mode = IMG_BILINEAR_FIXED): mixed
```

Returns a copy of an image scaled to a new size.

**Parameters**:
- `$image` (`mixed`)
- `$width` (`int`)
- `$height` (`int`), default `-1`, optional
- `$mode` (`int`), default `IMG_BILINEAR_FIXED`, optional

**Returns**: `mixed`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imagescale` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imagescale.md).
