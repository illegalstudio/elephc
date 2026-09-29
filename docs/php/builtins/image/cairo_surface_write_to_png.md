---
title: "cairo_surface_write_to_png()"
description: "Writes a surface to a PNG file."
sidebar:
  order: 446
---

## cairo_surface_write_to_png()

```php
function cairo_surface_write_to_png(mixed $surface, string $filename): void
```

Writes a surface to a PNG file.

**Parameters**:
- `$surface` (`mixed`)
- `$filename` (`string`)

**Returns**: `void`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `cairo_surface_write_to_png` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/cairo_surface_write_to_png.md).
