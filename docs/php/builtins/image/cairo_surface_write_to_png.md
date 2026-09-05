---
title: "cairo_surface_write_to_png()"
description: "Implemented by the compiler-injected image prelude."
sidebar:
  order: 443
---

## cairo_surface_write_to_png()

```php
function cairo_surface_write_to_png(mixed $surface, string $filename): void
```

Implemented by the compiler-injected image prelude.

**Parameters**:
- `$surface` (`mixed`)
- `$filename` (`string`)

**Returns**: `void`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `cairo_surface_write_to_png` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/cairo_surface_write_to_png.md).
