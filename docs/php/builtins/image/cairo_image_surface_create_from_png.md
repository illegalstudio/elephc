---
title: "cairo_image_surface_create_from_png()"
description: "Creates an image surface from a PNG file."
sidebar:
  order: 411
---

## cairo_image_surface_create_from_png()

```php
function cairo_image_surface_create_from_png(string $filename): mixed
```

Creates an image surface from a PNG file.

**Parameters**:
- `$filename` (`string`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `cairo_image_surface_create_from_png` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/cairo_image_surface_create_from_png.md).
