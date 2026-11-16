---
title: "imagecopyresized()"
description: "Copies and resizes a rectangle without interpolation."
---

## imagecopyresized()

```php
function imagecopyresized(mixed $dst_image, mixed $src_image, int $dst_x, int $dst_y, int $src_x, int $src_y, int $dst_width, int $dst_height, int $src_width, int $src_height): bool
```

Copies and resizes a rectangle without interpolation.

**Parameters**:
- `$dst_image` (`mixed`)
- `$src_image` (`mixed`)
- `$dst_x` (`int`)
- `$dst_y` (`int`)
- `$src_x` (`int`)
- `$src_y` (`int`)
- `$dst_width` (`int`)
- `$dst_height` (`int`)
- `$src_width` (`int`)
- `$src_height` (`int`)

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imagecopyresized` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imagecopyresized.md).
