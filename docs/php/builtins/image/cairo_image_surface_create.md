---
title: "cairo_image_surface_create()"
description: "Implemented by the compiler-injected image prelude."
sidebar:
  order: 407
---

## cairo_image_surface_create()

```php
function cairo_image_surface_create(int $format, int $width, int $height): mixed
```

Implemented by the compiler-injected image prelude.

**Parameters**:
- `$format` (`int`)
- `$width` (`int`)
- `$height` (`int`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `cairo_image_surface_create` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/cairo_image_surface_create.md).
