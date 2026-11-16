---
title: "imagefilledarc()"
description: "Draws a filled arc, pie slice, or chord."
---

## imagefilledarc()

```php
function imagefilledarc(mixed $image, int $center_x, int $center_y, int $width, int $height, int $start_angle, int $end_angle, int $color, int $style): bool
```

Draws a filled arc, pie slice, or chord.

**Parameters**:
- `$image` (`mixed`)
- `$center_x` (`int`)
- `$center_y` (`int`)
- `$width` (`int`)
- `$height` (`int`)
- `$start_angle` (`int`)
- `$end_angle` (`int`)
- `$color` (`int`)
- `$style` (`int`)

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imagefilledarc` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imagefilledarc.md).
