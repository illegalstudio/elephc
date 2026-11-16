---
title: "imagecropauto()"
description: "Crops an image automatically by the chosen mode."
---

## imagecropauto()

```php
function imagecropauto(mixed $image, int $mode = IMG_CROP_DEFAULT, float $threshold = 0.5, int $color = -1): mixed
```

Crops an image automatically by the chosen mode.

**Parameters**:
- `$image` (`mixed`)
- `$mode` (`int`), default `IMG_CROP_DEFAULT`, optional
- `$threshold` (`float`), default `0.5`, optional
- `$color` (`int`), default `-1`, optional

**Returns**: `mixed`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imagecropauto` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imagecropauto.md).
