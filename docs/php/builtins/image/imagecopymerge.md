---
title: "imagecopymerge()"
description: "Copies a rectangle into another image, blending it by a percentage."
---

## imagecopymerge()

```php
function imagecopymerge(mixed $dst_image, mixed $src_image, int $dst_x, int $dst_y, int $src_x, int $src_y, int $src_width, int $src_height, int $pct): bool
```

Copies a rectangle into another image, blending it by a percentage.

**Parameters**:
- `$dst_image` (`mixed`)
- `$src_image` (`mixed`)
- `$dst_x` (`int`)
- `$dst_y` (`int`)
- `$src_x` (`int`)
- `$src_y` (`int`)
- `$src_width` (`int`)
- `$src_height` (`int`)
- `$pct` (`int`)

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imagecopymerge` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imagecopymerge.md).
