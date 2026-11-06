---
title: "imagearc()"
description: "Draws an arc of an ellipse."
---

## imagearc()

```php
function imagearc(mixed $image, int $center_x, int $center_y, int $width, int $height, int $start_angle, int $end_angle, int $color): bool
```

Draws an arc of an ellipse.

**Parameters**:
- `$image` (`mixed`)
- `$center_x` (`int`)
- `$center_y` (`int`)
- `$width` (`int`)
- `$height` (`int`)
- `$start_angle` (`int`)
- `$end_angle` (`int`)
- `$color` (`int`)

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imagearc` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imagearc.md).
