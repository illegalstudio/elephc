---
title: "cairo_image_surface_create()"
description: "Creates an in-memory image surface of the given pixel format and size."
---

## cairo_image_surface_create()

```php
function cairo_image_surface_create(int $format, int $width, int $height): mixed
```

Creates an in-memory image surface of the given pixel format and size.

**Parameters**:
- `$format` (`int`)
- `$width` (`int`)
- `$height` (`int`)

**Returns**: `mixed`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `cairo_image_surface_create` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/cairo_image_surface_create.md).
